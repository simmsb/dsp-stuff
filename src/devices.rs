use cpal::traits::{DeviceTrait, HostTrait};
use rivulet::{SplittableView, View, ViewMut};

pub fn input_stream() -> (cpal::Stream, impl View<Item = f32>) {
    println!("{:#?}", cpal::available_hosts());

    let host = cpal::host_from_id(
        cpal::available_hosts()
            .into_iter()
            .find(|id| *id == cpal::HostId::Jack)
            .unwrap(),
    )
    .unwrap();

    for d in host.input_devices().unwrap() {
        println!("{:?}", d.name());
    }

    let dev = host.default_input_device().unwrap();
    let cfg = dev.default_input_config().unwrap();
    println!("{:#?}, {:#?}", cfg, cfg.config());

    let lowest_buf_size = match cfg.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max: _ } => *min,
        cpal::SupportedBufferSize::Unknown => 1024, // shrug
    };

    let mut cfg_v = cfg.config();

    cfg_v.buffer_size = match cfg_v.buffer_size {
        cpal::BufferSize::Default => cpal::BufferSize::Fixed(lowest_buf_size),
        x @ cpal::BufferSize::Fixed(_) => x,
    };

    println!("cfg: {:#?}", cfg_v);

    println!("using buf size: {}", lowest_buf_size);

    let (mut sink, source) = rivulet::circular_buffer::<f32>(lowest_buf_size as usize * 8);

    // TODO: sample type conversion

    let stream = dev
        .build_input_stream(
            &cfg_v,
            move |data: &[f32], _| {
                if sink.try_grant(data.len()).unwrap() {
                    let buf = sink.view_mut();
                    buf[..data.len()].copy_from_slice(data);
                    sink.release(data.len());
                } else {
                    println!("input fuck");
                    // input will fall behind
                };
            },
            move |err| {
                eprintln!("input oops: {:#?}", err);
            },
        )
        .unwrap();

    (stream, source.into_view())
}

pub fn output_stream() -> (cpal::Stream, impl ViewMut<Item = f32>) {
    let host = cpal::host_from_id(
        cpal::available_hosts()
            .into_iter()
            .find(|id| *id == cpal::HostId::Jack)
            .unwrap(),
    )
    .unwrap();

    for d in host.output_devices().unwrap() {
        println!("{:?}", d.name());
    }

    let dev = host.default_output_device().unwrap();
    let cfg = dev.default_output_config().unwrap();
    println!("{:#?}, {:#?}", cfg, cfg.config());

    let lowest_buf_size = match cfg.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max: _ } => *min,
        cpal::SupportedBufferSize::Unknown => 1024, // shrug
    };

    let mut cfg_v = cfg.config();

    cfg_v.buffer_size = match cfg_v.buffer_size {
        cpal::BufferSize::Default => cpal::BufferSize::Fixed(lowest_buf_size),
        x @ cpal::BufferSize::Fixed(_) => x,
    };

    println!("cfg: {:#?}", cfg_v);

    println!("using buf size: {}", lowest_buf_size);

    let (sink, source) = rivulet::circular_buffer::<f32>(lowest_buf_size as usize * 8);
    let mut source = source.into_view();

    let stream = dev
        .build_output_stream(
            &cfg_v,
            move |data: &mut [f32], _| {
                if source.try_grant(data.len()).unwrap() {
                    let buf = source.view();
                    data.copy_from_slice(&buf[..data.len()]);
                    source.release(data.len());
                } else {
                    println!("output fuck");
                    // oops
                };
            },
            move |err| {
                eprintln!("output oops: {:#?}", err);
            },
        )
        .unwrap();

    (stream, sink)
}
