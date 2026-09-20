#pragma once

/**
 * Return the number of short identification pulses in each three-second cycle.
 * Zero means the node ID is not assigned and must use the warning pattern.
 */
int identity_indicator_pulse_count(const char *node_id);
