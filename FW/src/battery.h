#pragma once

#include <stdbool.h>

void battery_init(void);
int battery_read_percent(void);

/** True once boot / plug / unplug ADC settle window has finished. */
bool battery_is_adc_settled(void);

/** USB cable connected to host (VBUS / USB-serial link). */
bool battery_is_usb_present(void);

/** Charger actively topping up: USB present and not yet charge-complete. */
bool battery_is_charge_active(void);

/** @deprecated Use battery_is_charge_active(). Kept for call-site clarity. */
bool battery_is_charging(void);
