# For GNU Make.

ZFONTSBDF = Zork_r400-10.bdf Zork_r400-11.bdf Zork_r400-13.bdf Zork_r400-16.bdf Zork_r400-20.bdf
ZFONTSPCF = $(ZFONTSBDF:.bdf=.pcf)

FONTDIR = $(DESTDIR)$(PREFIX)/share/fonts
X11_FONTDIR = $(FONTDIR)/X11
X11_ZORK_FONTDIR = $(X11_FONTDIR)/zork

MKFONTDIR ?= $(shell which mkfontdir)
XSET ?= $(shell which xset)

help:
	@echo "Font Targets:"
	@echo "         pcf: Build PCF fonts from BDF fonts"

pcf:	$(ZFONTSPCF)

install_pcf:	pcf
	@echo "Installing X11 fonts to $(X11_ZORK_FONTDIR)"
	mkdir -p $(X11_ZORK_FONTDIR)
	install -m 644 -t $(X11_ZORK_FONTDIR) $(ZFONTSPCF)
	$(MKFONTDIR) $(X11_ZORK_FONTDIR)
	$(XSET) +fp $(X11_ZORK_FONTDIR)

uninstall_pcf:
	rm -f $(X11_ZORK_FONTDIR)/*pcf
	rm -f $(X11_ZORK_FONTDIR)/fonts.dir
	# Remove as much as we added as we can.
	find $(FONTDIR) -depth -type d -empty -exec rmdir "{}" \;
	$(XSET) -fp $(X11_ZORK_FONTDIR)

clean:
	rm -f $(TARGET) $(OBJECTS) $(ZFONTSPCF)

.bdf.pcf:
	bdftopcf -t $< -o $@

.SUFFIXES: .bdf .pcf
.DELETE_ON_ERROR:
.PHONY:	clean install_pcf uninstall_pcf
