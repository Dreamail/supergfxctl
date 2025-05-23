VERSION := $(shell grep -Pm1 'version = "(\d.\d.\d)"' Cargo.toml | cut -d'"' -f2)

INSTALL = install
INSTALL_PROGRAM = ${INSTALL} -D -m 0755
INSTALL_DATA = ${INSTALL} -D -m 0644

prefix = /usr
exec_prefix = $(prefix)
bindir = $(exec_prefix)/bin
datarootdir = $(prefix)/share
libdir = $(exec_prefix)/lib

BIN_SD := supergfxd
BIN_SC := supergfxctl
SERVICE := supergfxd.service
PRESET := supergfxd.preset
DBUSCFG := org.supergfxctl.Daemon.conf
X11CFG := 90-nvidia-screen-G05.conf
PMRULES := 90-supergfxd-nvidia-pm.rules

SRC := Cargo.toml Cargo.lock Makefile $(shell find -type f -wholename '**/src/*.rs')

DEBUG ?= 0
ifeq ($(DEBUG),0)
	ARGS += --release
	TARGET = release
endif

VENDORED ?= 0
ifeq ($(VENDORED),1)
	ARGS += --frozen
endif

all: build

clean:
	cargo clean

distclean:
	rm -rf .cargo vendor vendor-$(VERSION).tar.xz

install:
	$(INSTALL_PROGRAM) "./target/release/$(BIN_SD)" "$(DESTDIR)$(bindir)/$(BIN_SD)"
	$(INSTALL_PROGRAM) "./target/release/$(BIN_SC)" "$(DESTDIR)$(bindir)/$(BIN_SC)"
	$(INSTALL_DATA) "./data/$(SERVICE)" "$(DESTDIR)$(libdir)/systemd/system/$(SERVICE)"
	$(INSTALL_DATA) "./data/$(PRESET)" "$(DESTDIR)$(libdir)/systemd/system-preset/$(PRESET)"
	$(INSTALL_DATA) "./data/$(DBUSCFG)" "$(DESTDIR)$(datarootdir)/dbus-1/system.d/$(DBUSCFG)"
	$(INSTALL_DATA) "./data/$(X11CFG)" "$(DESTDIR)$(datarootdir)/X11/xorg.conf.d/$(X11CFG)"
	$(INSTALL_DATA) "./data/$(PMRULES)" "$(DESTDIR)$(libdir)/udev/rules.d/$(PMRULES)"

uninstall:
	rm -f "$(DESTDIR)$(bindir)/$(BIN_SC)"
	rm -f "$(DESTDIR)$(bindir)/$(BIN_SD)"
	rm -f "$(DESTDIR)$(libdir)/systemd/system/$(SERVICE)"
	rm -f "$(DESTDIR)$(libdir)/systemd/system-preset/$(PRESET)"
	rm -f "$(DESTDIR)$(datarootdir)/dbus-1/system.d/org.supergfxctl.Daemon.conf"
	rm -f "$(DESTDIR)$(datarootdir)/X11/xorg.conf.d/$(X11CFG)"
	rm -f "$(DESTDIR)$(libdir)/udev/rules.d/$(PMRULES)"

update:
	cargo update

vendor:
	mkdir -p .cargo
	cargo vendor-filterer --platform x86_64-unknown-linux-gnu vendor
	tar pcfJ vendor-$(VERSION).tar.xz vendor
	rm -rf vendor

build:
ifeq ($(VENDORED),1)
	@echo "version = $(VERSION)"
	tar pxf vendor-$(VERSION).tar.xz
endif
	cargo build --features "daemon cli" $(ARGS)
	strip -s ./target/release/$(BIN_SD)
	strip -s ./target/release/$(BIN_SC)

.PHONY: all clean distclean install uninstall update build
