# what it says on the tin 
run:
  cargo build --release
  env PIPEWIRE_LATENCY=128/48000 WINIT_UNIX_BACKEND=x11 ./target/release/dsp-stuff

# what it says on the tin 
run_clear:
  cargo build --release
  env PIPEWIRE_LATENCY=128/48000 WINIT_UNIX_BACKEND=x11 ./target/release/dsp-stuff -c
