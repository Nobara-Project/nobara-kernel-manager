# Nobara Kernel Manager

Nobara Kernel Manager is a small Rust/libadwaita frontend for switching between
Nobara's Mainline and LTS kernel repositories and rebuilding the rescue kernel.
It is derived from
[CosmicFusion/fedora-kernel-manager](https://github.com/CosmicFusion/fedora-kernel-manager)
and remains licensed under MPL-2.0.

## Behavior

- Reads Boot Loader Specification entries under `/boot/loader/entries` to
  identify installed kernel families.
- Shows the booted kernel, installed rescue kernel, and the latest available
  Mainline and LTS kernel versions.
- Disables the button for the installed Nobara kernel family.
- Disables both switching buttons when any third-party kernel entry is present.
- Leaves **Reinstall Rescue Kernel** available regardless of kernel detection.
- Runs all kernel-changing operations through PolicyKit and the bundled
  `kernel-manager` backend.

Switching removes all installed versions of the managed kernel packages before
installing the latest packages from the selected Nobara kernel source. Rescue
reinstallation uses the newest bootable kernel matching the enabled Nobara
kernel source, builds a strict host-only initramfs, excludes external modules,
uses basic graphics mode, and keeps the rescue entry last in GRUB.

## Build

```bash
make build
```

## Install into a staging root

```bash
make install_no_build DESTDIR=/tmp/nobara-kernel-manager-root
```

## Commands used by the frontend

```text
kernel-manager switch mainline
kernel-manager switch lts
kernel-manager rescue reinstall
```
