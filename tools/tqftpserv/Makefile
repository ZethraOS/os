# SPDX-License-Identifier: BSD-3-Clause
OUT := tqftpserv

CC ?= aarch64-linux-gnu-gcc
CFLAGS ?= -Wall -g -O2 -D_GNU_SOURCE
CFLAGS += -I. -I../qrtr/include
LDFLAGS += -static

SRCS := tqftpserv.c translate.c ../qrtr/lib/qrtr.c ../qrtr/lib/logging.c
OBJS := tqftpserv.o translate.o ../qrtr/lib/qrtr.o ../qrtr/lib/logging.o

all: $(OUT)

$(OUT): $(OBJS)
	$(CC) -o $@ $^ $(LDFLAGS)

%.o: %.c
	$(CC) $(CFLAGS) -c -o $@ $<

clean:
	rm -f $(OUT) tqftpserv.o translate.o
