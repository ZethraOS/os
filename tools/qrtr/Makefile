# SPDX-License-Identifier: BSD-3-Clause
CC ?= gcc
CFLAGS ?= -O2 -Wall
CFLAGS += -Iinclude -Isrc
LDFLAGS += -static -lpthread

LIB_SRCS := lib/logging.c lib/qmi.c lib/qrtr.c
NS_SRCS := src/addr.c src/hash.c src/map.c src/ns.c src/util.c src/waiter.c
LOOKUP_SRCS := src/lookup.c

BINS := qrtr-ns qrtr-lookup

all: $(BINS)

qrtr-ns: $(LIB_SRCS) $(NS_SRCS)
	$(CC) $(CFLAGS) -o $@ $^ $(LDFLAGS)

qrtr-lookup: $(LIB_SRCS) $(LOOKUP_SRCS)
	$(CC) $(CFLAGS) -o $@ $^ $(LDFLAGS)

clean:
	rm -f $(BINS)
