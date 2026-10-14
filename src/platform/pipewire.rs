//! Microphone capture from the default PipeWire source.

use anyhow::{Context, Result};
use pipewire as pw;
use pw::{properties::properties, spa};
use spa::pod::Pod;
use std::cell::RefCell;
use std::rc::Rc;
use std::thread::JoinHandle;

use super::{Capture, CaptureEvent};
use crate::state::MicFailure;

#[derive(Default)]
pub struct PipewireCapture {
    stop: Option<pw::channel::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

type Sink = Box<dyn FnMut(CaptureEvent) + Send>;
type SharedSink = Rc<RefCell<Sink>>;

impl Capture for PipewireCapture {
    fn start(&mut self, sink: Sink) -> Result<()> {
        self.stop();
        let (stop_tx, stop_rx) = pw::channel::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("pipewire-capture".into())
            .spawn(move || {
                let sink = Rc::new(RefCell::new(sink));
                if let Err(e) = run(sink.clone(), stop_rx) {
                    tracing::error!("pipewire capture failed: {e}");
                    (sink.borrow_mut())(CaptureEvent::Failed(MicFailure::NoMicrophone));
                }
            })
            .context("spawning capture thread")?;
        self.stop = Some(stop_tx);
        self.thread = Some(thread);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for PipewireCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run(sink: SharedSink, stop: pw::channel::Receiver<()>) -> Result<(), pw::Error> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;

    let props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Communication",
        *pw::keys::APP_NAME => "wisprs",
    };
    let stream = pw::stream::StreamBox::new(&core, "wisprs", props)?;

    // Both callbacks run on this thread's loop.
    let on_state = sink.clone();
    let _listener = stream
        .add_local_listener_with_user_data(())
        .state_changed(move |_, _, _, state| match state {
            pw::stream::StreamState::Streaming => (on_state.borrow_mut())(CaptureEvent::Streaming),
            pw::stream::StreamState::Error(message) => {
                tracing::error!("stream error: {message}");
                (on_state.borrow_mut())(CaptureEvent::Failed(MicFailure::NoMicrophone));
            }
            _ => {}
        })
        .process(move |stream, _| {
            let Some(mut buffer) = stream.dequeue_buffer() else { return };
            let Some(data) = buffer.datas_mut().first_mut() else { return };
            let bytes = data.chunk().size() as usize;
            let Some(raw) = data.data() else { return };
            let samples: Vec<f32> = raw[..bytes.min(raw.len())]
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            if !samples.is_empty() {
                (sink.borrow_mut())(CaptureEvent::Samples(samples));
            }
        })
        .register()?;

    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
    audio_info.set_rate(crate::spectrogram::SAMPLE_RATE as u32);
    audio_info.set_channels(1);
    let object = spa::pod::Object {
        type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: spa::param::ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };
    let bytes = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(object),
    )
    .map_err(|_| pw::Error::CreationFailed)?
    .0
    .into_inner();
    let mut params = [Pod::from_bytes(&bytes).ok_or(pw::Error::CreationFailed)?];

    stream.connect(
        spa::utils::Direction::Input,
        None,
        pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS | pw::stream::StreamFlags::RT_PROCESS,
        &mut params,
    )?;

    let quit = mainloop.clone();
    let _stop = stop.attach(mainloop.loop_(), move |()| quit.quit());
    mainloop.run();
    // Disconnect before returning so the node is gone when `stop()` returns.
    stream.disconnect()?;
    Ok(())
}
