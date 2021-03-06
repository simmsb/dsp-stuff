# DSP Stuff

A node based audio effects thing.

![image](https://user-images.githubusercontent.com/5330444/150704818-13342938-4914-413a-899f-fd975bdba5ab.png)


# Usage

Run with `cargo run --release --features gpl_effects` (customize the feature
flags as you wish)

If you're using pipewire, you can set the `PIPEWIRE_LATENCY` env var to force
pipewire to give us the lowest latency possible, for example:

```sh
env PIPEWIRE_LATENCY=128/48000 cargo run --release
```

## Feature flags

- `gpl_effects`: Enables building with effects that are gpl licensed. (note:
  this will make the built binary gpl licensed)
- `windows`: Enables building cpal with ASIO support
- `console`: Enables the tokio console subscriber

## Plumbing

If you're on linux, the JACK interface of cpal seems to work by creating a
source/sink pair for the application. You'll want to use something like
qjackctl, or [pw-viz](https://github.com/Ax9D/pw-viz/tree/grouped_nodes) to
manage connecting up these interfaces.

## Buffer sizes

Currently the device handling is rather primitive.

The current implementation uses cpal's 'default' buffer size option. I tried
opening devices with the buffer size set to the lowest size specified in the
config range, but alsa seems to lie or just fail when you try to set the buffer
size on some/all devices?

If you're using pipewire you can use the PIPEWIRE_LATENCY env var to lock the
buffer sizes.

## Notes

- This currently assumes the sample rate is 48000hz
