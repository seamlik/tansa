# Tansa: Serverless LAN service discovery over IPv6

[![codecov](https://codecov.io/github/seamlik/tansa/graph/badge.svg?token=AH6YGZOMJM)](https://codecov.io/github/seamlik/tansa)

This project is an opinionated solution of service discovery within a IPv6-enabled LAN.
Written in pure Rust, it is [cross-platform](https://doc.rust-lang.org/nightly/rustc/platform-support.html) and is suitable to be a unified and portable solution for an application planning to span all major devices like laptops, smartphones and more.
Unlike solutions such as [DNS Service Discovery](http://dns-sd.org),
applications using this solution do not rely on any server/daemon running on the operating system.

This project offers both a crate and a CLI application.
However, I will not publish it to any public registry since it is highly experimental.

## Technologies under the hood

### Multicast

Service discovery is mostly implemented by exchanging multicast packets on a rendezvous address.

### IP neighbors

When multicast is unavailable, services on other devices are naturally undiscoverable.
However, devices connected directly to the host device are still discoverable as IP neighbors.

This scenario can happen, for example, when 2 Android smartphones are connected via a hotspot or even Wi-Fi Direct.
In this case, no other device exists within the LAN, but applications on the smartphones can still communicate.

This feature relies on the operating system.
Current we support these operating systems:

- Windows (using [PowerShell](https://learn.microsoft.com/powershell/module/nettcpip/get-netneighbor?view=windowsserver2022-ps))
- Linux (using [iproute2](https://wiki.linuxfoundation.org/networking/iproute2))

## Project name meaning

Based on Japanese "探索" with pronounciation "tansaku" meaning "discovery".
