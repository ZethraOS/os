# SPDX-License-Identifier: BSD-3-Clause
OUT := rmtfs

CC ?= gcc
CFLAGS ?= -Wall -g -O2
CFLAGS += -I. -I../qrtr/include -DANDROID
LDFLAGS += -static -lpthread

prefix = /usr/local
bindir := $(prefix)/bin
servicedir := $(prefix)/lib/systemd/system
rulesdir := $(prefix)/lib/udev/rules.d

RMTFS_EFS_PATH ?= /var/lib/rmtfs

SRCS := qmi_rmtfs.c rmtfs.c rproc.c sharedmem.c storage.c util.c \
        ../qrtr/lib/qrtr.c ../qrtr/lib/qmi.c ../qrtr/lib/logging.c
OBJS := $(SRCS:.c=.o)

all: $(OUT)

$(OUT): $(OBJS)
	$(CC) -o $@ $^ $(LDFLAGS)

%.o: %.c
	$(CC) $(CFLAGS) -c -o $@ $<

clean:
	rm -f $(OUT) $(OBJS)
