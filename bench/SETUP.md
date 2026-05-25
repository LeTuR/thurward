# `bench/SETUP.md` — first-run setup on Arch Linux

One-time install of the QEMU/KVM + libvirt substrate. Run this once on
a fresh workstation; subsequent benchmark runs use the `make` targets
documented in [`README.md`](./README.md).

> If you're on a non-Arch distro, translate package names and service
> names accordingly. The harness only assumes `qemu-system-x86_64`,
> `libvirtd`, `virsh`, `virt-install`, `cloud-localds`, and the
> `libvirt` / `kvm` user groups.

## 1. Hardware sanity check

```
[ -e /dev/kvm ] && echo "KVM ok" || echo "no /dev/kvm — enable VT-x/AMD-V in firmware"
```

You need at least 8 GiB free RAM and ~10 GiB free disk for the cached
images + per-run instance copies.

## 2. Install packages

```
sudo pacman -S --needed \
    qemu-full libvirt virt-install \
    dnsmasq bridge-utils iproute2 \
    cloud-image-utils edk2-ovmf swtpm \
    jq libguestfs guestfs-tools iperf3 \
    tcpdump
```

`qemu-full` pulls in everything; on slim setups `qemu-base` +
`qemu-system-x86` is enough.

## 3. Enable libvirt

```
sudo systemctl enable --now libvirtd.socket
sudo systemctl enable --now virtlogd.socket
```

## 3a. Run qemu as your user

`qemu:///system` defaults to running the qemu process as the
`libvirt-qemu` system user — which can't traverse `/home/<you>` to read
the bench instance disks stored under `bench/images/instances/`. Tell
libvirt to run qemu as your user instead:

```
sudo tee -a /etc/libvirt/qemu.conf >/dev/null <<EOF

# thurward-bench: run qemu as the invoking user so instance qcow2s
# stored under /home/<user>/Repositories/.../bench/images/instances/
# are readable by the qemu process.
user = "$USER"
group = "kvm"
EOF
sudo systemctl restart libvirtd.service
```

Without this, `make sub-up` fails with
`error: Cannot access storage file ... (as uid:955, gid:955): Permission denied`.

## 4. User groups

Your shell user needs to be in `libvirt` and `kvm`:

```
sudo usermod -aG libvirt,kvm "$USER"
# log out and back in, or open a new shell:
newgrp libvirt
```

Verify:

```
id | tr , '\n' | grep -E 'libvirt|kvm'
virsh -c qemu:///system net-list --all
```

If `virsh net-list` errors with "no connection", the group membership
hasn't taken effect — open a fresh terminal.

## 5. (Optional, Tier-0 → Tier-1) Hugepages

TRex (Phase 3) and VPP (later candidate) want hugepages. The Tier-0
smoke test doesn't need them — skip until you reach the TRex pass.

When you do need them:

```
echo 1024 | sudo tee /proc/sys/vm/nr_hugepages       # 2 GiB of 2 MiB hugepages
# Persist:
echo 'vm.nr_hugepages = 1024' | sudo tee /etc/sysctl.d/60-bench-hugepages.conf
```

## 6. Generate the harness SSH key

The first `make` invocation will do this for you, but you can run it
manually:

```
mkdir -p bench/keys
ssh-keygen -t ed25519 -N '' -f bench/keys/harness -C 'thurward-bench-harness'
```

This key is **harness-only** — it boots into every SUT VM via
cloud-init. It is **not** added to `~/.ssh/`; it lives only in
`bench/keys/` and is `.gitignore`d.

## 7. (Recommended) Cache the Debian base image

`make sub-up CAND=nftables` will download the Debian 12 cloud image on
first run, but you can pre-cache it:

```
mkdir -p bench/images/base
curl -L -o bench/images/base/debian-12-genericcloud-amd64.qcow2 \
    https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2
```

Checksums are verified by the Makefile on every run.

## 8. Smoke test the install

```
cd bench
make net-up
virsh -c qemu:///system net-list
# should show lan-thurward and wan-thurward, both active
make net-down
```

If both bridges came up, you're ready.

## Troubleshooting

| Symptom                                                          | Fix                                                                                                  |
| ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `virsh: error: failed to connect to the hypervisor`              | Group membership hasn't taken effect; open a fresh shell or `newgrp libvirt`.                        |
| `Could not access KVM kernel module: Permission denied`          | User not in `kvm` group, or `/dev/kvm` permissions are wrong (`ls -l /dev/kvm` should be `0660 kvm`). |
| Wayland + libvirt graphical viewer doesn't open                  | Use `virsh console <vm>` (text) instead of `virt-viewer`. The harness doesn't need a GUI.            |
| `network 'lan-thurward' is not active` after host reboot         | Networks are not auto-started; run `make net-up` again.                                              |
| nftables guest can't reach package mirrors                       | Cloud-init runs *after* the firewall ruleset loads; the guest is intentionally isolated. The Makefile pre-bakes all packages via `make base-customise` — re-run that target if you change `BASE_PACKAGES`. |
| `error: Cannot access storage file ... (as uid:955, gid:955)`    | Run § 3a — `qemu:///system` is still running qemu as `libvirt-qemu`. Bench disks live under `/home`, which that user can't traverse. |
| Guest boots but `localhost login:` instead of `thur-*`           | Cloud-init never read the seed. The harness injects the seed into `/var/lib/cloud/seed/nocloud/` on the instance qcow2 via `virt-customize`; if this step fails (e.g. `guestfs-tools` missing) cloud-init falls back to default state. Check `make sub-up` output. |
