#include <assert.h>
#include <stdio.h>

#include "identity_indicator.h"

int main(void)
{
    assert(identity_indicator_pulse_count("MMWAVE1") == 1);
    assert(identity_indicator_pulse_count("MMWAVE2") == 2);
    assert(identity_indicator_pulse_count("MMWAVE3") == 0);
    assert(identity_indicator_pulse_count("") == 0);
    assert(identity_indicator_pulse_count(NULL) == 0);
    puts("mmWave identity indicator tests passed");
    return 0;
}
