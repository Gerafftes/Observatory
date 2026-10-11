#include <assert.h>
#include <stdint.h>
#include <string.h>

#include "tx_packet.h"
#include "tx_scheduler.h"

static uint32_t read_u32(const uint8_t *bytes)
{
    uint32_t value;
    memcpy(&value, bytes, sizeof(value));
    return value;
}

static uint64_t read_u64(const uint8_t *bytes)
{
    uint64_t value;
    memcpy(&value, bytes, sizeof(value));
    return value;
}

int main(void)
{
    uint8_t packet[TX_SOUNDING_PACKET_SIZE];
    tx_packet_build(packet, 17, 1234, UINT64_C(0x0102030405060708), 6, 50);
    assert(sizeof(packet) == 32);
    assert(memcmp(packet, "CSI_TX", 6) == 0);
    assert(packet[6] == 2 && packet[7] == 2);
    assert(read_u32(packet + 8) == 17);
    assert(read_u32(packet + 12) == 1234);
    assert(memcmp(packet + 16, "TX1", 3) == 0);
    assert(read_u64(packet + 20) == UINT64_C(0x0102030405060708));
    assert(packet[28] == 6 && packet[29] == 50);
    const uint8_t expected[32] = {
        'C','S','I','_','T','X',2,2,17,0,0,0,0xd2,4,0,0,
        'T','X','1',0,8,7,6,5,4,3,2,1,6,50,0,0
    };
    assert(memcmp(packet, expected, sizeof(expected)) == 0);

    tx_scheduler_t scheduler;
    tx_scheduler_init(&scheduler, 50, 1000000);
    tx_schedule_result_t delayed = tx_scheduler_on_wakeup(&scheduler, 3, 1060100);
    assert(delayed.sequence == 0);
    assert(delayed.missed_this_wakeup == 2);
    assert(scheduler.missed_deadlines == 2);
    tx_schedule_result_t next = tx_scheduler_on_wakeup(&scheduler, 1, 1080000);
    assert(next.sequence == 1);
    assert(next.missed_this_wakeup == 0);
    assert(scheduler.missed_deadlines == 2);
    return 0;
}
