# Makefile for Unix Frotz
# GNU make is required

SOURCES = blorblib.c

HEADERS = blorb.h blorblow.h

OBJECTS = $(SOURCES:.c=.o)

TARGET = blorblib.a

ARFLAGS = rc

.PHONY: clean
.DELETE_ON_ERROR:

$(TARGET): $(OBJECTS)
	$(AR) $(ARFLAGS) $@ $?
	$(RANLIB) $@
	@echo "** Done with Blorb library."

clean:
	rm -f $(TARGET) $(OBJECTS)

%.o: %.c
	$(CC) $(CFLAGS) -I../common -fPIC -fpic -o $@ -c $<
