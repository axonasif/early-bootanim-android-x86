# Intruduction
--------------

This is a really simple re-implementaion of https://github.com/Mortal/fbspinner for android-x86.

This intends to render an animated bootlogo asynchronously at the early boot stages of initrd.img.


# Build instructions
--------------------

* First of all you need rust. Follow the link below and you will know what to do.

> It'll also autoinstall rustc and cargo for you.

> https://www.rust-lang.org/tools/install

* Once you have rust and everything it brings, we add a new target. Run the following command:

```bash
rustup target add x86_64-unknown-linux-musl
```

> Or if you wan to make for 32bit arch then the target should be `i686-unknown-linux-musl`.

* After you have the build targets installed simply run the following command:

```bash
RUSTFLAGS='-C link-arg=-s' cargo build --release --target x86_64-unknown-linux-musl  # For 64bit

# or

RUSTFLAGS='-C link-arg=-s' cargo build --release --target i686-unknown-linux-musl    # For 32bit
```

> Then you should find the output as `target/x86_64-unknown-linux-musl/release/early-bootanim` (64bit)


***more todo stuff .....***



## How to make custom anim.bin
---------------------------

> Make sure you have python3 and pip installed.
> Also ensure that you have required python modules,
> if you don't then run the following command: `pip3 install imageio`

* Put your png frames at `anim` dir.

* While being in the `early-bootanim` dir run the following command:
```
python3 scripts/flatten.py
```

# Note
----

This project is still **WIP**


# Licence
---------

This project is licenced under **GPL-3.0 License**
