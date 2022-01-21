# DSP Stuff

A node based audio effects thing.

![image](https://user-images.githubusercontent.com/5330444/149669648-914e02dc-f744-4153-8e05-c7edb8530233.png)


# Usage

Run with `cargo run --release`

If you're using pipewire, you can set the `PIPEWIRE_LATENCY` env var to force
pipewire to give us the lowest latency possible, for example:

```sh
env PIPEWIRE_LATENCY=128/48000 cargo run --release
```


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
