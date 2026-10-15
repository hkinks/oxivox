//! wlr-layer-shell overlay: a click-through surface that never takes keyboard focus.
//! It exists only while the overlay is visible; hidden means no surface and no rendering.

use anyhow::{Context, Result, anyhow};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, FrameCallbackData, Region};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::{self, EventLoop};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::client::globals::registry_queue_init;
use smithay_client_toolkit::reexports::client::protocol::{wl_output, wl_shm, wl_surface};
use smithay_client_toolkit::reexports::client::{Connection, Dispatch, QueueHandle};
use smithay_client_toolkit::reexports::protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use smithay_client_toolkit::reexports::protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::{
    self, WpFractionalScaleV1,
};
use smithay_client_toolkit::reexports::protocols::wp::viewporter::client::wp_viewport::WpViewport;
use smithay_client_toolkit::reexports::protocols::wp::viewporter::client::wp_viewporter::WpViewporter;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_registry, registry_handlers};
use std::sync::Arc;
use std::time::Instant;
use tiny_skia::Pixmap;

use crate::overlay::{Overlay, OverlayCmd, OverlaySink, Surface};

#[derive(Default)]
pub struct LayerShellSurface;

struct Sink(calloop::channel::Sender<OverlayCmd>);

impl OverlaySink for Sink {
    fn send(&self, cmd: OverlayCmd) {
        let _ = self.0.send(cmd);
    }
}

impl Surface for LayerShellSurface {
    fn spawn(self: Box<Self>, overlay: Overlay) -> Result<Arc<dyn OverlaySink>> {
        let (commands, receiver) = calloop::channel::channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        std::thread::Builder::new().name("overlay".into()).spawn(move || match setup(overlay, receiver) {
            Ok((mut event_loop, mut app)) => {
                let _ = ready_tx.send(Ok(()));
                while !app.exit {
                    if let Err(e) = event_loop.dispatch(None, &mut app) {
                        tracing::error!("overlay event loop failed: {e}");
                        break;
                    }
                }
            }
            Err(e) => {
                let _ = ready_tx.send(Err(e));
            }
        })?;
        ready_rx.recv().context("overlay thread died")??;
        Ok(Arc::new(Sink(commands)))
    }
}

fn setup(overlay: Overlay, commands: calloop::channel::Channel<OverlayCmd>) -> Result<(EventLoop<'static, App>, App)> {
    let conn = Connection::connect_to_env().context("connecting to the Wayland compositor")?;
    let (globals, event_queue) = registry_queue_init(&conn).context("listing Wayland globals")?;
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).context("wl_compositor is missing")?;
    let layer_shell = LayerShell::bind(&globals, &qh).context("the compositor does not support wlr-layer-shell")?;
    let shm = Shm::bind(&globals, &qh).context("wl_shm is missing")?;
    let pool = SlotPool::new(1 << 16, &shm).context("creating shm pool")?;
    // Without both, the overlay falls back to the compositor's integer scale.
    let fractional_scale = globals.bind::<WpFractionalScaleManagerV1, _, _>(&qh, 1..=1, ()).ok();
    let viewporter = globals.bind::<WpViewporter, _, _>(&qh, 1..=1, ()).ok();

    let event_loop: EventLoop<App> = EventLoop::try_new()?;
    WaylandSource::new(conn, event_queue).insert(event_loop.handle()).map_err(|e| anyhow!("{e}"))?;
    event_loop
        .handle()
        .insert_source(commands, |event, _, app| {
            if let calloop::channel::Event::Msg(cmd) = event {
                app.overlay.apply(cmd, Instant::now());
                app.sync();
            }
        })
        .map_err(|e| anyhow!("{e}"))?;

    let app = App {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        compositor,
        layer_shell,
        shm,
        pool,
        fractional_scale,
        viewporter,
        fractional: None,
        viewport: None,
        qh,
        overlay,
        layer: None,
        configured: false,
        frame_pending: false,
        scale: 1.0,
        pixmap: None,
        exit: false,
    };
    Ok((event_loop, app))
}

struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    pool: SlotPool,
    fractional_scale: Option<WpFractionalScaleManagerV1>,
    viewporter: Option<WpViewporter>,
    /// Per-surface objects of the fractional scale protocol; they live as long as the layer.
    fractional: Option<WpFractionalScaleV1>,
    viewport: Option<WpViewport>,
    qh: QueueHandle<App>,
    overlay: Overlay,
    layer: Option<LayerSurface>,
    configured: bool,
    frame_pending: bool,
    /// Output scale, 1.5 for 150 %; whole numbers unless the compositor speaks fractional-scale-v1.
    scale: f32,
    pixmap: Option<Pixmap>,
    exit: bool,
}

impl App {
    /// Brings the surface in line with the overlay: create when it became visible, destroy when hidden,
    /// and draw straight away so a state change does not wait for the next frame callback.
    fn sync(&mut self) {
        match (self.overlay.is_visible(), self.layer.is_some()) {
            (true, false) => self.create_layer(),
            (false, true) => self.destroy_layer(),
            (true, true) if self.configured && !self.frame_pending => self.draw(),
            _ => {}
        }
    }

    fn create_layer(&mut self) {
        let surface = self.compositor.create_surface(&self.qh);
        let (width, height) = self.overlay.size();
        let layer = self.layer_shell.create_layer_surface(&self.qh, surface, Layer::Overlay, Some("oxivox"), None);
        layer.set_anchor(Anchor::BOTTOM);
        layer.set_margin(0, 0, self.overlay.config().bottom_margin, 0);
        layer.set_size(width, height);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        // An empty input region lets clicks fall through to the window below.
        match Region::new(&self.compositor) {
            Ok(region) => layer.wl_surface().set_input_region(Some(region.wl_region())),
            Err(e) => tracing::warn!("cannot make surface click-through: {e}"),
        }
        if let (Some(manager), Some(viewporter)) = (&self.fractional_scale, &self.viewporter) {
            // The buffer is sized in device pixels and mapped onto the logical size by the viewport.
            self.fractional = Some(manager.get_fractional_scale(layer.wl_surface(), &self.qh, ()));
            let viewport = viewporter.get_viewport(layer.wl_surface(), &self.qh, ());
            viewport.set_destination(width as i32, height as i32);
            self.viewport = Some(viewport);
        } else {
            layer.wl_surface().set_buffer_scale(self.scale as i32);
        }
        layer.commit();
        self.layer = Some(layer);
        self.configured = false;
        self.frame_pending = false;
    }

    fn destroy_layer(&mut self) {
        if let Some(fractional) = self.fractional.take() {
            fractional.destroy();
        }
        if let Some(viewport) = self.viewport.take() {
            viewport.destroy();
        }
        self.layer = None;
        self.configured = false;
        self.frame_pending = false;
    }

    fn draw(&mut self) {
        let now = Instant::now();
        self.overlay.tick(now);
        if !self.overlay.is_visible() {
            self.destroy_layer();
            return;
        }
        let Some(layer) = &self.layer else { return };
        let (width, height) = self.overlay.size();
        let (pw, ph) = ((width as f32 * self.scale).round() as u32, (height as f32 * self.scale).round() as u32);
        let pixmap = match &mut self.pixmap {
            Some(p) if p.width() == pw && p.height() == ph => p,
            slot => slot.insert(Pixmap::new(pw, ph).expect("overlay size is non-zero")),
        };
        self.overlay.render(pixmap, now);

        let stride = pw as i32 * 4;
        let (buffer, canvas) = match self.pool.create_buffer(pw as i32, ph as i32, stride, wl_shm::Format::Argb8888) {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("cannot allocate overlay buffer: {e}");
                return;
            }
        };
        // tiny-skia stores premultiplied RGBA; wl_shm Argb8888 is premultiplied BGRA in memory.
        for (dst, src) in canvas.chunks_exact_mut(4).zip(pixmap.data().chunks_exact(4)) {
            dst.copy_from_slice(&[src[2], src[1], src[0], src[3]]);
        }
        let surface = layer.wl_surface();
        surface.damage_buffer(0, 0, pw as i32, ph as i32);
        surface.frame(&self.qh, FrameCallbackData(surface.clone()));
        self.frame_pending = true;
        if buffer.attach_to(surface).is_err() {
            tracing::error!("cannot attach overlay buffer");
        }
        layer.commit();
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        if self.viewport.is_some() {
            return;
        }
        self.scale = factor.max(1) as f32;
        surface.set_buffer_scale(factor.max(1));
        if self.configured {
            self.draw();
        }
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        self.frame_pending = false;
        if self.layer.is_some() {
            self.draw();
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        // The compositor took the surface away (output removed); the next Show creates a new one.
        self.destroy_layer();
    }

    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface, _: LayerSurfaceConfigure, _: u32) {
        self.configured = true;
        self.draw();
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_registry!(App);

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

smithay_client_toolkit::delegate_dispatch2!(App);

impl Dispatch<WpFractionalScaleV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let wp_fractional_scale_v1::Event::PreferredScale { scale } = event else { return };
        let scale = scale as f32 / 120.0;
        if scale != app.scale {
            app.scale = scale;
            if app.configured {
                app.draw();
            }
        }
    }
}

smithay_client_toolkit::reexports::client::delegate_noop!(App: WpFractionalScaleManagerV1);
smithay_client_toolkit::reexports::client::delegate_noop!(App: WpViewporter);
smithay_client_toolkit::reexports::client::delegate_noop!(App: WpViewport);
