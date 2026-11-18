#pragma once

#include <stddef.h>
#include <stdint.h>

void protocol_init(void);
void protocol_feed_rx(const uint8_t *data, size_t len);
void protocol_update(void);
