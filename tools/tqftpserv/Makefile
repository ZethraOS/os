# SPDX-License-Identifier: BSD-3-Clause
OUT := tqftpserv

CC ?= aarch64-linux-gnu-gcc
CFLAGS ?= -Wall -Os -ffunction-sections -fdata-sections -D_GNU_SOURCE
CFLAGS += -I. -I../qrtr/include
LDFLAGS += -Wl,--gc-sections -static

SRCS := tqftpserv.c translate.c ../qrtr/lib/qrtr.c ../qrtr/lib/logging.c
OBJS := tqftpserv.o translate.o qrtr.o logging.o

all: $(OUT)

$(OUT): $(OBJS)
	$(CC) -o $@ $^ $(LDFLAGS)

qrtr.o: ../qrtr/lib/qrtr.c
	$(CC) $(CFLAGS) -c -o $@ $<

logging.o: ../qrtr/lib/logging.c
	$(CC) $(CFLAGS) -c -o $@ $<

%.o: %.c
	$(CC) $(CFLAGS) -c -o $@ $<

clean:
	rm -f $(OUT) *.o
