#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "node_identity.h"

int main(void)
{
    const uint8_t expected_rgb[4][3] = {
        {10, 132, 255},
        {48, 209, 88},
        {255, 55, 95},
        {191, 90, 242},
    };
    node_identity_t identities[4];

    for (uint8_t wire_id = 1; wire_id <= 4; ++wire_id) {
        assert(node_identity_resolve(wire_id, &identities[wire_id - 1]));
        char expected_label[4] = {'R', 'X', (char)('0' + wire_id), '\0'};
        assert(strcmp(identities[wire_id - 1].label, expected_label) == 0);
        assert(identities[wire_id - 1].red == expected_rgb[wire_id - 1][0]);
        assert(identities[wire_id - 1].green == expected_rgb[wire_id - 1][1]);
        assert(identities[wire_id - 1].blue == expected_rgb[wire_id - 1][2]);
    }

    for (size_t first = 0; first < 4; ++first) {
        for (size_t second = first + 1; second < 4; ++second) {
            assert(identities[first].red != identities[second].red
                   || identities[first].green != identities[second].green
                   || identities[first].blue != identities[second].blue);
        }
    }

    node_identity_t unknown = {.label = "stale", .red = 1, .green = 1, .blue = 1};
    assert(!node_identity_resolve(0, &unknown));
    assert(unknown.label == NULL);
    assert(!node_identity_resolve(5, &unknown));
    assert(!node_identity_resolve(1, NULL));

    puts("node identity tests passed");
    return 0;
}
