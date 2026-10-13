# Makefile for Unix Frotz
# GNU make is required

SOURCES = dblorb.c dinit.c dinput.c doutput.c dpic.c

OBJECTS = $(SOURCES:.c=.o)

TARGET = frotz_dumb.a

ARFLAGS = rc

.PHONY: clean
.DELETE_ON_ERROR:

$(TARGET): $(OBJECTS)
	$(AR) $(ARFLAGS) $@ $?
	$(RANLIB) $@
	@echo "** Done with dumb interface."

clean:
	rm -f $(TARGET) $(OBJECTS)

%.o: %.c
	$(CC) $(CFLAGS) -fPIC -fpic -o $@ -c $<
