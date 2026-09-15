# Build and install targets for Dota 2 Stats.
#
# Two independent artifacts share this file:
#   dota-stats      the Tauri desktop app (GUI)
#   dota-stats-cli  the terminal client
#
# Default PREFIX is a per-user install, so no target here needs root. Override
# it for a system install (the PKGBUILD does exactly that):
#   make install PREFIX=/usr DESTDIR=/tmp/pkgdir

PREFIX  ?= $(HOME)/.local
DESTDIR ?=

BINDIR     := $(PREFIX)/bin
LIBDIR     := $(PREFIX)/lib/dota-stats
APPDIR     := $(PREFIX)/share/applications
ICONDIR    := $(PREFIX)/share/icons/hicolor
CARGO      ?= cargo
TARGETDIR  := target/release

.PHONY: all build install install-gui install-cli \
        uninstall uninstall-gui uninstall-cli package clean help

all: build

help:
	@echo "build          compile both binaries in release mode"
	@echo "install        install GUI and CLI into $(PREFIX)"
	@echo "install-gui    install only the desktop app"
	@echo "install-cli    install only the terminal client"
	@echo "uninstall      remove everything from $(PREFIX)"
	@echo "package        build an Arch package with makepkg"
	@echo "clean          cargo clean"

build:
	$(CARGO) build --release

install: install-gui install-cli

# GUI: the real binary goes to LIBDIR and a detaching wrapper takes the
# `dota-stats` name on PATH, so launching from a terminal returns the prompt.
install-gui: build
	install -Dm755 $(TARGETDIR)/dota-stats $(DESTDIR)$(LIBDIR)/dota-stats
	install -Dm755 packaging/dota-stats.sh $(DESTDIR)$(BINDIR)/dota-stats
	install -Dm644 app/icons/32x32.png   $(DESTDIR)$(ICONDIR)/32x32/apps/dota-stats.png
	install -Dm644 app/icons/128x128.png $(DESTDIR)$(ICONDIR)/128x128/apps/dota-stats.png
	install -d $(DESTDIR)$(APPDIR)
	sed 's|@BINDIR@|$(BINDIR)|g' packaging/dota-stats.desktop \
		> $(DESTDIR)$(APPDIR)/dota-stats.desktop
	chmod 644 $(DESTDIR)$(APPDIR)/dota-stats.desktop
	@# Skipped when staging into DESTDIR: the package manager owns that step.
	@if [ -z "$(DESTDIR)" ] && command -v update-desktop-database >/dev/null; then \
		update-desktop-database -q $(APPDIR) || true; \
	fi

install-cli: build
	install -Dm755 $(TARGETDIR)/dota-stats-cli $(DESTDIR)$(BINDIR)/dota-stats-cli

uninstall: uninstall-gui uninstall-cli

uninstall-gui:
	rm -f $(DESTDIR)$(BINDIR)/dota-stats
	rm -rf $(DESTDIR)$(LIBDIR)
	rm -f $(DESTDIR)$(APPDIR)/dota-stats.desktop
	rm -f $(DESTDIR)$(ICONDIR)/32x32/apps/dota-stats.png
	rm -f $(DESTDIR)$(ICONDIR)/128x128/apps/dota-stats.png

uninstall-cli:
	rm -f $(DESTDIR)$(BINDIR)/dota-stats-cli

package:
	cd packaging && makepkg -f

clean:
	$(CARGO) clean
