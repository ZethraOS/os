/*
 * fih_skuid_emulator.c — ZethraOS Phase 5K
 *
 * Emulates Android RIL's qcril_oem_request_hook_raw_fih_skuid() over
 * AF_QIPCRTR to deliver SKUID1 to the Hexagon modem before
 * fih_nv_qmi_skuid2() fires its comparison and err_fatal().
 *
 * Built with libqrtr.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <unistd.h>
#include <errno.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <poll.h>

#include <signal.h>

#include <libqrtr.h>
#include <time.h>
#include <linux/qrtr.h>

static volatile int running = 1;

static void cleanup(int sig)
{
    (void)sig;
    running = 0;
}

#define FIH_SERVICE_ID   15       /* 0x000F — QMI_PING_SVC / FIH OEM  */
#define FIH_MSG_ID       6        /* 0x0006 — FIH_COMM_QMI_REQ_V01    */
#define FIH_SUBCMD_SKUID 0x01
#define SKUID_STR        "600WW"
#define REQ_PAYLOAD_LEN  260      /* fixed 260-byte body (from disasm) */
#define RETRY_SEC        30

#pragma pack(push, 1)
struct qmi_header {
    uint8_t  type;      /* 0x00 = request */
    uint16_t txn_id;
    uint16_t msg_id;
    uint16_t msg_len;   /* length of payload AFTER this header */
};
#pragma pack(pop)

static uint16_t le16(uint16_t v)
{
#if __BYTE_ORDER__ == __ORDER_BIG_ENDIAN__
    return __builtin_bswap16(v);
#else
    return v;
#endif
}

static int send_fih_skuid(int sock, uint32_t node, uint32_t port,
                           const char *skuid)
{
    uint8_t wire[sizeof(struct qmi_header) + REQ_PAYLOAD_LEN];
    struct qmi_header *hdr = (struct qmi_header *)wire;
    uint8_t *body = wire + sizeof(struct qmi_header);
    size_t skuid_len = strlen(skuid);
    uint32_t payload_len = 1 + skuid_len + 1;   /* subcmd + str + NUL */
    int rc;

    memset(wire, 0, sizeof(wire));

    /* QMI header */
    hdr->type   = 0x00;                 /* QMI_REQUEST */
    hdr->txn_id = le16(0x0001);
    hdr->msg_id = le16(FIH_MSG_ID);
    hdr->msg_len = le16(REQ_PAYLOAD_LEN);

    /* FIH payload */
    body[0] = (uint8_t)(payload_len & 0xFF);         /* LE uint32_t */
    body[1] = (uint8_t)((payload_len >> 8) & 0xFF);
    body[2] = (uint8_t)((payload_len >> 16) & 0xFF);
    body[3] = (uint8_t)((payload_len >> 24) & 0xFF);
    body[4] = FIH_SUBCMD_SKUID;
    strncpy((char *)&body[5], skuid, REQ_PAYLOAD_LEN - 6);

    printf("[fih_skuid] Sending FIH_COMM_QMI_REQ_V01 (msg_id=%d) "
           "SKUID=\"%s\" payload_len=%u -> node=%u port=%u\n",
           FIH_MSG_ID, skuid, payload_len, node, port);

    rc = qrtr_sendto(sock, node, port, wire, sizeof(wire));
    if (rc < 0) {
        perror("[fih_skuid] qrtr_sendto failed");
        return -1;
    }
    printf("[fih_skuid] Sent %zu bytes to node %u port %u\n", sizeof(wire), node, port);
    return 0;
}

int main(int argc, char **argv)
{
    /* Disable stdio buffering so log writes immediately */
    setvbuf(stdout, NULL, _IONBF, 0);
    setvbuf(stderr, NULL, _IONBF, 0);

    printf("[fih_skuid] Starting FIH SKUID Emulator (built with libqrtr)\n");
    printf("[fih_skuid] Target SKUID: %s, target service: %d\n", SKUID_STR, FIH_SERVICE_ID);

    int sock = qrtr_open(0);
    if (sock < 0) {
        fprintf(stderr, "[fih_skuid] Failed to open QRTR socket\n");
        return 1;
    }

    /* Register lookups with in-kernel nameserver */
    printf("[fih_skuid] Registering lookup for service %d and wildcard\n", FIH_SERVICE_ID);
    qrtr_new_lookup(sock, FIH_SERVICE_ID, 0, 0);
    qrtr_new_lookup(sock, 0, 0, 0);

    signal(SIGTERM, cleanup);
    signal(SIGINT, cleanup);

    int sent_count = 0;
    char buf[4096];
    struct sockaddr_qrtr from;
    socklen_t sl;

    while (1) {
        if (!running)
            break;
        int pr = qrtr_poll(sock, 1000);
        if (pr > 0) {
            sl = sizeof(from);
            int len = recvfrom(sock, buf, sizeof(buf), 0, (struct sockaddr *)&from, &sl);
            if (len > 0) {
                struct qrtr_packet pkt;
                if (qrtr_decode(&pkt, buf, len, &from) == 0) {
                    if (pkt.type == QRTR_TYPE_NEW_SERVER) {
                        printf("[fih_skuid] NEW_SERVER: service=%u instance=%u node=%u port=%u\n",
                               pkt.service, pkt.instance, pkt.node, pkt.port);
                        if (pkt.service == FIH_SERVICE_ID) {
                            printf("[fih_skuid] *** MATCHED FIH SERVICE %u at %u:%u! ***\n",
                                   pkt.service, pkt.node, pkt.port);
                            send_fih_skuid(sock, pkt.node, pkt.port, SKUID_STR);
                            printf("[fih_skuid] Sent SKUID response: %s\n", SKUID_STR);
                            sent_count++;
                        }
                    } else if (pkt.type == QRTR_TYPE_DEL_SERVER) {
                        printf("[fih_skuid] DEL_SERVER: service=%u instance=%u node=%u port=%u\n",
                               pkt.service, pkt.instance, pkt.node, pkt.port);
                    } else if (pkt.type == QRTR_TYPE_BYE) {
                        printf("[fih_skuid] BYE from node=%u\n", pkt.node);
                    } else if (pkt.type == QRTR_TYPE_DEL_CLIENT) {
                        printf("[fih_skuid] DEL_CLIENT: node=%u port=%u\n", pkt.node, pkt.port);
                    } else if (pkt.type == QRTR_TYPE_DATA) {
                        printf("[fih_skuid] DATA (%zu bytes) from node=%u port=%u\n",
                               pkt.data_len, pkt.node, pkt.port);
                    }
                }
            }
        } else {
            sleep(1);
        }
    }

    qrtr_close(sock);
    printf("[fih_skuid] Finished (sent_count=%d)\n", sent_count);
    return 0;
}
