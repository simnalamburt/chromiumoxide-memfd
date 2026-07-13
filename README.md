chromiumoxide-memfd
========

```bash
cargo run
```

How to cross-compile on macOS:
```bash
# Install musl toolchain
brew install filosottile/musl-cross/musl-cross

rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl x86_64-apple-darwin aarch64-apple-darwin

cargo build -r --target x86_64-unknown-linux-musl
cargo build -r --target aarch64-unknown-linux-musl
cargo build -r --target aarch64-apple-darwin
cargo build -r --target x86_64-apple-darwin

mkdir -p target/universal2-apple-darwin/release
lipo -create -output target/{universal2,aarch64,x86_64}-apple-darwin/release/chromiumoxide-memfd

ls target/{universal2-apple-darwin,{x86_64,aarch64}-unknown-linux-musl}/release/chromiumoxide-memfd
```

Run on Linux:
```
# Install runtime dependencies
sudo apt-get install -y libnss3 libnspr4 libexpat1 libc6 libgcc-s1

./chromiumoxide-memfd
```
