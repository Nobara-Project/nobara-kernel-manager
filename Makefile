PREFIX ?= /usr
LIBDIR ?= $(PREFIX)/lib/nobara-kernel-manager
DATADIR ?= $(PREFIX)/share

.PHONY: all build build_debug install install_no_build install_no_build_debug install_data

all: build

build:
	cargo build --release

build_debug:
	cargo build

install: build install_no_build

install_no_build:
	install -D -m 0755 target/release/nobara-kernel-manager \
		$(DESTDIR)$(PREFIX)/bin/nobara-kernel-manager
	$(MAKE) install_data DESTDIR=$(DESTDIR) PREFIX=$(PREFIX) LIBDIR=$(LIBDIR) DATADIR=$(DATADIR)

install_no_build_debug:
	install -D -m 0755 target/debug/nobara-kernel-manager \
		$(DESTDIR)$(PREFIX)/bin/nobara-kernel-manager
	$(MAKE) install_data DESTDIR=$(DESTDIR) PREFIX=$(PREFIX) LIBDIR=$(LIBDIR) DATADIR=$(DATADIR)

install_data:
	install -D -m 0755 data/scripts/kernel-manager \
		$(DESTDIR)$(LIBDIR)/kernel-manager
	install -D -m 0755 data/scripts/kernel-status \
		$(DESTDIR)$(LIBDIR)/kernel-status
	install -D -m 0644 data/com.github.cosmicfusion.nobara-kernel-manager.desktop \
		$(DESTDIR)$(DATADIR)/applications/com.github.cosmicfusion.nobara-kernel-manager.desktop
	install -D -m 0644 data/com.github.cosmicfusion.nobara-kernel-manager.svg \
		$(DESTDIR)$(DATADIR)/icons/hicolor/scalable/apps/com.github.cosmicfusion.nobara-kernel-manager.svg
	install -D -m 0644 data/nobara-kernel-manager.1 \
		$(DESTDIR)$(DATADIR)/man/man1/nobara-kernel-manager.1
	install -D -m 0644 data/polkit-1/actions/org.nobaraproject.kernel-manager.manage.policy \
		$(DESTDIR)$(DATADIR)/polkit-1/actions/org.nobaraproject.kernel-manager.manage.policy
	install -D -m 0644 data/polkit-1/actions/org.nobaraproject.kernel-manager.status.policy \
		$(DESTDIR)$(DATADIR)/polkit-1/actions/org.nobaraproject.kernel-manager.status.policy
