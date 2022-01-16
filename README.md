# DSP Stuff

A node based audio effects thing.

![image](https://user-images.githubusercontent.com/5330444/149646015-60b63fb0-472a-4076-b48f-4ef6c3a70693.png)


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

## Notes

- This currently assumes the sample rate is 48000hz
