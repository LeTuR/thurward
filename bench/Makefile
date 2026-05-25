# bench/Makefile — orchestration for the QEMU/KVM benchmark harness.
#
# See README.md for the topology + Tier-0 trade-offs and SETUP.md for
# the one-time install on a fresh Arch box.
#
# Conventions:
#   - All libvirt operations go through `qemu:///system` so they survive
#     shell sessions and run under the libvirt service account, not the
#     user shell.
#   - VMs are named thur-<role>-<candidate> e.g. thur-sut-nftables,
#     thur-gen. The `thur-` prefix matches the bridge names so it's
#     obvious which networks they belong to.
#   - Per-run state lives under bench/images/instances/ and is
#     destroyed by `make tear-down`. The base image cache in
#     bench/images/base/ is preserved.

SHELL := /bin/bash
.ONESHELL:
.SHELLFLAGS := -euo pipefail -c

# ---------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------

BENCH_DIR        := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
IMAGES_BASE_DIR  := $(BENCH_DIR)/images/base
IMAGES_INST_DIR  := $(BENCH_DIR)/images/instances
KEYS_DIR         := $(BENCH_DIR)/keys
RESULTS_DIR      := $(BENCH_DIR)/results
SCRIPTS_DIR      := $(BENCH_DIR)/scripts

# Debian 12 generic cloud image (~330 MB).
BASE_IMAGE_URL   := https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2
BASE_IMAGE       := $(IMAGES_BASE_DIR)/debian-12-genericcloud-amd64.qcow2

HARNESS_KEY      := $(KEYS_DIR)/harness
HARNESS_KEY_PUB  := $(HARNESS_KEY).pub

LIBVIRT_URI      := qemu:///system

# Default candidate; override on the command line: `make sub-up CAND=pf`.
CAND ?= nftables

# Today's results subdir for smoke runs.
TODAY            := $(shell date -u +%Y-%m-%dT%H-%M-%SZ)
SMOKE_RESULT_DIR := $(RESULTS_DIR)/smoke/$(TODAY)

# ---------------------------------------------------------------------
# Help (default target)
# ---------------------------------------------------------------------

.DEFAULT_GOAL := help

.PHONY: help
help:
	@cat <<'EOF'
	bench/ — QEMU/KVM benchmark harness for thurward

	One-time setup (see SETUP.md):
	   sudo pacman -S --needed qemu-full libvirt virt-install \
	                            dnsmasq bridge-utils cloud-image-utils \
	                            edk2-ovmf swtpm jq libguestfs iperf3
	   sudo systemctl enable --now libvirtd.socket
	   sudo usermod -aG libvirt,kvm "$$USER"      # then `newgrp libvirt`

	Common workflow:
	   make net-up                         bring up the two libvirt bridges
	   make sub-up    CAND=nftables        boot the firewall SUT
	   make gen-up                         boot the traffic generator
	   make smoke     CAND=nftables        run the reachability smoke test
	   make sub-shell CAND=nftables        SSH into the SUT
	   make tear-down                      destroy everything

	Status:
	   make status                         what is running right now

	Reset (forces image re-download next run):
	   make distclean
	EOF

# ---------------------------------------------------------------------
# One-time helpers
# ---------------------------------------------------------------------

$(HARNESS_KEY):
	@mkdir -p $(KEYS_DIR)
	@chmod 700 $(KEYS_DIR)
	@if [ ! -f $@ ]; then \
	  ssh-keygen -t ed25519 -N '' -f $@ -C 'thurward-bench-harness' >/dev/null; \
	  echo "Generated harness SSH key at $@"; \
	fi

.PHONY: keys
keys: $(HARNESS_KEY)

$(BASE_IMAGE):
	@mkdir -p $(IMAGES_BASE_DIR)
	@if [ ! -f $@ ]; then \
	  echo "Downloading Debian 12 cloud image (~330 MB)..."; \
	  curl -fL -o $@.tmp $(BASE_IMAGE_URL); \
	  mv $@.tmp $@; \
	fi
	@ls -lh $@

.PHONY: base-image
base-image: $(BASE_IMAGE)

# Packages pre-baked into the cached base image. The bench bridges are
# `forward mode='none'` (no external NAT), so guest VMs cannot reach
# package mirrors at cloud-init time. virt-customize installs the union
# of tools every candidate / generator needs before the image is ever
# cloned. A sentinel file inside the image records that customisation
# ran, so re-runs are idempotent and don't double-install.
BASE_PACKAGES := nftables,tcpdump,iperf3,conntrack,iputils-ping,iputils-arping,jq,iproute2,python3
BASE_CUSTOM_MARK := /var/lib/thurward-bench.customised

$(BASE_IMAGE).customised: $(BASE_IMAGE)
	@if ! command -v virt-customize >/dev/null 2>&1; then \
	  echo "ERROR: virt-customize not found (install guestfs-tools)"; exit 1; \
	fi
	@if sudo virt-cat -a $(BASE_IMAGE) $(BASE_CUSTOM_MARK) >/dev/null 2>&1; then \
	  echo "  base image already customised"; \
	else \
	  echo "  customising base image with: $(BASE_PACKAGES)"; \
	  sudo virt-customize -a $(BASE_IMAGE) \
	    --install $(BASE_PACKAGES) \
	    --write '/etc/cloud/cloud.cfg.d/99-thurward-nocloud.cfg:datasource_list: [ NoCloud, None ]' \
	    --truncate /etc/machine-id \
	    --run-command 'mkdir -p /var/lib && date -u +%FT%TZ > $(BASE_CUSTOM_MARK)'; \
	fi
	@touch $@

.PHONY: base-customise base-customize
base-customise base-customize: $(BASE_IMAGE).customised

# ---------------------------------------------------------------------
# Networks
# ---------------------------------------------------------------------

.PHONY: net-up
net-up:
	@for net in lan-thurward wan-thurward; do \
	  if ! virsh -c $(LIBVIRT_URI) net-info $$net >/dev/null 2>&1; then \
	    virsh -c $(LIBVIRT_URI) net-define $(BENCH_DIR)/topology/$$net.xml; \
	  fi; \
	  if [ "$$(virsh -c $(LIBVIRT_URI) net-info $$net | awk '/^Active:/ {print $$2}')" != "yes" ]; then \
	    virsh -c $(LIBVIRT_URI) net-start $$net; \
	  fi; \
	  echo "  network $$net: up"; \
	done

.PHONY: net-down
net-down:
	@for net in lan-thurward wan-thurward; do \
	  if virsh -c $(LIBVIRT_URI) net-info $$net >/dev/null 2>&1; then \
	    if [ "$$(virsh -c $(LIBVIRT_URI) net-info $$net | awk '/^Active:/ {print $$2}')" = "yes" ]; then \
	      virsh -c $(LIBVIRT_URI) net-destroy $$net; \
	    fi; \
	    virsh -c $(LIBVIRT_URI) net-undefine $$net; \
	  fi; \
	  echo "  network $$net: down"; \
	done

# ---------------------------------------------------------------------
# Per-candidate SUT lifecycle
# ---------------------------------------------------------------------

# Per-instance paths.
SUT_DISK     = $(IMAGES_INST_DIR)/thur-sut-$(CAND).qcow2
SUT_NAME     = thur-sut-$(CAND)
SUT_DIR      = $(BENCH_DIR)/candidates/$(CAND)
SUT_USER     = bench

.PHONY: sub-up
sub-up: net-up $(BASE_IMAGE).customised $(HARNESS_KEY)
	@if [ ! -d $(SUT_DIR) ]; then echo "ERROR: $(SUT_DIR) does not exist (CAND=$(CAND))"; exit 1; fi
	@mkdir -p $(IMAGES_INST_DIR)
	@if [ ! -f $(SUT_DISK) ]; then \
	  echo "Cloning base image to instance: $(SUT_DISK)"; \
	  qemu-img create -f qcow2 -F qcow2 -b $(BASE_IMAGE) $(SUT_DISK) 20G; \
	  $(SCRIPTS_DIR)/render-cloud-init.sh $(CAND) sut $(SUT_DISK) $(HARNESS_KEY_PUB); \
	fi
	@if ! virsh -c $(LIBVIRT_URI) dominfo $(SUT_NAME) >/dev/null 2>&1; then \
	  echo "Defining $(SUT_NAME)"; \
	  virt-install --connect $(LIBVIRT_URI) \
	    --name $(SUT_NAME) \
	    --memory 2048 --vcpus 1 \
	    --cpu host-passthrough \
	    --osinfo debian12 \
	    --disk path=$(SUT_DISK),format=qcow2,bus=virtio \
	    --network network=lan-thurward,model=virtio \
	    --network network=wan-thurward,model=virtio \
	    --graphics none --console pty,target_type=serial \
	    --import --noautoconsole --noreboot; \
	fi
	@if [ "$$(virsh -c $(LIBVIRT_URI) domstate $(SUT_NAME))" != "running" ]; then \
	  virsh -c $(LIBVIRT_URI) start $(SUT_NAME); \
	fi
	@echo "  $(SUT_NAME): running (LAN 10.10.0.1, WAN 203.0.113.1)"

.PHONY: sub-down
sub-down:
	@if virsh -c $(LIBVIRT_URI) dominfo $(SUT_NAME) >/dev/null 2>&1; then \
	  virsh -c $(LIBVIRT_URI) destroy $(SUT_NAME) 2>/dev/null || true; \
	  virsh -c $(LIBVIRT_URI) undefine $(SUT_NAME) --nvram 2>/dev/null || true; \
	  echo "  $(SUT_NAME): down"; \
	fi

.PHONY: sub-shell
sub-shell:
	@ssh -i $(HARNESS_KEY) \
	     -o StrictHostKeyChecking=no \
	     -o UserKnownHostsFile=/dev/null \
	     -o LogLevel=ERROR \
	     $(SUT_USER)@10.10.0.1

# ---------------------------------------------------------------------
# Traffic generator lifecycle
# ---------------------------------------------------------------------

GEN_DISK := $(IMAGES_INST_DIR)/thur-gen.qcow2
GEN_NAME := thur-gen

.PHONY: gen-up
gen-up: net-up $(BASE_IMAGE).customised $(HARNESS_KEY)
	@mkdir -p $(IMAGES_INST_DIR)
	@if [ ! -f $(GEN_DISK) ]; then \
	  qemu-img create -f qcow2 -F qcow2 -b $(BASE_IMAGE) $(GEN_DISK) 20G; \
	  $(SCRIPTS_DIR)/render-cloud-init.sh trafficgen gen $(GEN_DISK) $(HARNESS_KEY_PUB); \
	fi
	@if ! virsh -c $(LIBVIRT_URI) dominfo $(GEN_NAME) >/dev/null 2>&1; then \
	  virt-install --connect $(LIBVIRT_URI) \
	    --name $(GEN_NAME) \
	    --memory 2048 --vcpus 1 \
	    --cpu host-passthrough \
	    --osinfo debian12 \
	    --disk path=$(GEN_DISK),format=qcow2,bus=virtio \
	    --network network=lan-thurward,model=virtio \
	    --network network=wan-thurward,model=virtio \
	    --graphics none --console pty,target_type=serial \
	    --import --noautoconsole --noreboot; \
	fi
	@if [ "$$(virsh -c $(LIBVIRT_URI) domstate $(GEN_NAME))" != "running" ]; then \
	  virsh -c $(LIBVIRT_URI) start $(GEN_NAME); \
	fi
	@echo "  $(GEN_NAME): running (LAN 10.10.0.100, WAN 203.0.113.100)"

.PHONY: gen-down
gen-down:
	@if virsh -c $(LIBVIRT_URI) dominfo $(GEN_NAME) >/dev/null 2>&1; then \
	  virsh -c $(LIBVIRT_URI) destroy $(GEN_NAME) 2>/dev/null || true; \
	  virsh -c $(LIBVIRT_URI) undefine $(GEN_NAME) --nvram 2>/dev/null || true; \
	  echo "  $(GEN_NAME): down"; \
	fi

# ---------------------------------------------------------------------
# WAN-side sink VM (for TRex B-NN: avoids same-VM loopback shortcut)
# ---------------------------------------------------------------------

SINK_DISK    := $(IMAGES_INST_DIR)/thur-sink.qcow2
SINK_NAME    := thur-sink
SINK_WAN_IP  := 203.0.113.200

.PHONY: sink-up
sink-up: net-up $(BASE_IMAGE).customised $(HARNESS_KEY)
	@mkdir -p $(IMAGES_INST_DIR)
	@if [ ! -f $(SINK_DISK) ]; then \
	  echo "Cloning base image to sink instance"; \
	  qemu-img create -f qcow2 -F qcow2 -b $(BASE_IMAGE) $(SINK_DISK) 20G; \
	  $(SCRIPTS_DIR)/render-cloud-init.sh sink gen $(SINK_DISK) $(HARNESS_KEY_PUB); \
	fi
	@if ! virsh -c $(LIBVIRT_URI) dominfo $(SINK_NAME) >/dev/null 2>&1; then \
	  virt-install --connect $(LIBVIRT_URI) \
	    --name $(SINK_NAME) \
	    --memory 1024 --vcpus 1 \
	    --cpu host-passthrough \
	    --osinfo debian12 \
	    --disk path=$(SINK_DISK),format=qcow2,bus=virtio \
	    --network network=wan-thurward,model=virtio \
	    --graphics none --console pty,target_type=serial \
	    --import --noautoconsole --noreboot; \
	fi
	@if [ "$$(virsh -c $(LIBVIRT_URI) domstate $(SINK_NAME))" != "running" ]; then \
	  virsh -c $(LIBVIRT_URI) start $(SINK_NAME); \
	fi
	@echo "  $(SINK_NAME): running (WAN $(SINK_WAN_IP))"

.PHONY: sink-down
sink-down:
	@if virsh -c $(LIBVIRT_URI) dominfo $(SINK_NAME) >/dev/null 2>&1; then \
	  virsh -c $(LIBVIRT_URI) destroy $(SINK_NAME) 2>/dev/null || true; \
	  virsh -c $(LIBVIRT_URI) undefine $(SINK_NAME) --nvram 2>/dev/null || true; \
	  echo "  $(SINK_NAME): down"; \
	fi

# ---------------------------------------------------------------------
# TRex generator lifecycle (replaces gen-up for RFC-9411 measurements)
# ---------------------------------------------------------------------

TREX_TARBALL := $(BENCH_DIR)/images/trex/trex-latest.tar.gz
TREX_DISK    := $(IMAGES_INST_DIR)/thur-trex.qcow2
TREX_NAME    := thur-trex
TREX_LAN_IP  := 10.10.0.200

# Fetch TRex tarball (~270 MB). Cisco's CDN doesn't send the
# intermediate cert in the TLS chain, so we use -k and verify the
# tarball is a valid gzip afterwards.
$(TREX_TARBALL):
	@mkdir -p $(dir $@)
	@if [ ! -f $@ ]; then \
	  echo "Downloading TRex tarball (~270 MB)..."; \
	  curl -fSL -k --connect-timeout 30 -o $@.tmp \
	    https://trex-tgn.cisco.com/trex/release/latest; \
	  file $@.tmp | grep -q 'gzip compressed' || { echo "ERROR: not a gzip"; rm $@.tmp; exit 1; }; \
	  mv $@.tmp $@; \
	fi
	@ls -lh $@

.PHONY: trex-fetch
trex-fetch: $(TREX_TARBALL)

.PHONY: trex-up
trex-up: net-up $(BASE_IMAGE).customised $(HARNESS_KEY) $(TREX_TARBALL)
	@mkdir -p $(IMAGES_INST_DIR)
	@if [ ! -f $(TREX_DISK) ]; then \
	  echo "Cloning base image to TRex instance"; \
	  qemu-img create -f qcow2 -F qcow2 -b $(BASE_IMAGE) $(TREX_DISK) 30G; \
	  $(SCRIPTS_DIR)/render-cloud-init.sh trex gen $(TREX_DISK) $(HARNESS_KEY_PUB); \
	fi
	@if ! virsh -c $(LIBVIRT_URI) dominfo $(TREX_NAME) >/dev/null 2>&1; then \
	  echo "Defining $(TREX_NAME)"; \
	  virt-install --connect $(LIBVIRT_URI) \
	    --name $(TREX_NAME) \
	    --memory 4096 --vcpus 4 \
	    --cpu host-passthrough \
	    --osinfo debian12 \
	    --disk path=$(TREX_DISK),format=qcow2,bus=virtio \
	    --network network=lan-thurward,model=virtio \
	    --network network=wan-thurward,model=virtio \
	    --memorybacking hugepages=on \
	    --graphics none --console pty,target_type=serial \
	    --import --noautoconsole --noreboot; \
	fi
	@if [ "$$(virsh -c $(LIBVIRT_URI) domstate $(TREX_NAME))" != "running" ]; then \
	  virsh -c $(LIBVIRT_URI) start $(TREX_NAME); \
	fi
	@echo "  $(TREX_NAME): running (LAN $(TREX_LAN_IP))"

.PHONY: trex-down
trex-down:
	@if virsh -c $(LIBVIRT_URI) dominfo $(TREX_NAME) >/dev/null 2>&1; then \
	  virsh -c $(LIBVIRT_URI) destroy $(TREX_NAME) 2>/dev/null || true; \
	  virsh -c $(LIBVIRT_URI) undefine $(TREX_NAME) --nvram 2>/dev/null || true; \
	  echo "  $(TREX_NAME): down"; \
	fi

# B-00: reference path, empty ruleset. SUT is booted in pass-through
# (no rules.nft loaded — see candidates/nftables/rules-empty.nft).
B00_RESULT_DIR := $(RESULTS_DIR)/b-00/$(TODAY)

.PHONY: b-00
b-00: trex-up sink-up sub-up
	@mkdir -p $(B00_RESULT_DIR)
	@$(SCRIPTS_DIR)/b00.sh $(CAND) $(B00_RESULT_DIR) $(HARNESS_KEY) $(TREX_LAN_IP) $(SINK_WAN_IP)
	@echo "  result: $(B00_RESULT_DIR)/result.json"

# ---------------------------------------------------------------------
# Smoke test (Tier-0 reachability + iperf3 sanity)
# ---------------------------------------------------------------------

.PHONY: smoke
smoke: sub-up gen-up
	@mkdir -p $(SMOKE_RESULT_DIR)
	@$(SCRIPTS_DIR)/smoke.sh $(CAND) $(SMOKE_RESULT_DIR) $(HARNESS_KEY)
	@echo "  result: $(SMOKE_RESULT_DIR)/result.json"

# ---------------------------------------------------------------------
# Status / introspection / teardown
# ---------------------------------------------------------------------

.PHONY: status
status:
	@echo "=== networks ==="
	@virsh -c $(LIBVIRT_URI) net-list --all
	@echo
	@echo "=== domains ==="
	@virsh -c $(LIBVIRT_URI) list --all

.PHONY: tear-down
tear-down: sub-down gen-down trex-down sink-down net-down
	@echo "  harness fully torn down"

.PHONY: distclean
distclean: tear-down
	@rm -rf $(IMAGES_INST_DIR)
	@echo "  instance images removed; base image cache and keys preserved"
