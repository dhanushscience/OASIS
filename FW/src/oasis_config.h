#pragma once

// Headless build: no OLED on device (max battery life).
#define OASIS_HAS_DISPLAY 0

// Set to 1 only while diagnosing power/BLE state over UART telemetry.
#define OASIS_POWER_DEBUG_BLE_TELEMETRY 0
