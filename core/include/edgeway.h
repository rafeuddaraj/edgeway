/*
 * Edgeway core — C interface for platform shells.
 *
 * Copyright (c) 2026 Rafe Uddaraj. MIT License.
 *
 * Mirrors core/src/ffi.rs; keep the two in sync. Callbacks run on engine
 * threads, never on the caller's UI thread.
 */

#ifndef EDGEWAY_H
#define EDGEWAY_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Roles */
#define EW_ROLE_PRIMARY 1   /* shares this device's keyboard and pointer */
#define EW_ROLE_SECONDARY 2 /* is controlled by another device */

/* EwEvent.kind */
#define EW_EVENT_MOUSE_MOVE 1
#define EW_EVENT_BUTTON 2
#define EW_EVENT_SCROLL 3
#define EW_EVENT_KEY 4
#define EW_EVENT_OTHER 5 /* gestures etc.: swallowed while another device has focus */
#define EW_EVENT_WAKE 6  /* injection only: keep the display awake */

/* Neutral modifier bits (EwEvent.mods) */
#define EW_MOD_SHIFT (1u << 0)
#define EW_MOD_CONTROL (1u << 1)
#define EW_MOD_ALT (1u << 2)
#define EW_MOD_META (1u << 3)
#define EW_MOD_CAPS_LOCK (1u << 4)
#define EW_MOD_FN (1u << 5)

/* Control callback operations (primary) */
#define EW_CONTROL_FREEZE 1   /* freeze and hide the cursor at (x, y) */
#define EW_CONTROL_UNFREEZE 2 /* show the cursor again at (x, y) */

/* Pairing callback kinds */
#define EW_PAIRING_SHOW_CODE 1  /* secondary: show `detail` (the code) large */
#define EW_PAIRING_ENTER_CODE 2 /* primary: ask for the code shown on `peer` */
#define EW_PAIRING_SUCCEEDED 3
#define EW_PAIRING_FAILED 4 /* `detail` is a sentence for the user */
#define EW_PAIRING_CLOSED 5 /* dismiss any pairing UI */

typedef struct EwEngine EwEngine;

/* One input event, captured (primary) or to inject (secondary). */
typedef struct EwEvent {
    uint8_t kind;
    uint8_t button; /* 0 left, 1 right, 2 middle, 3 back, 4 forward */
    uint8_t down;
    uint8_t clicks;
    uint8_t repeat;
    uint8_t phase;    /* scroll phase: 0 none, 1 began, 2 changed, 3 ended, 4 cancelled, 5 may-begin */
    uint8_t momentum; /* momentum phase, same values */
    uint8_t continuous;
    uint16_t key; /* USB HID usage */
    int16_t lines_x;
    int16_t lines_y;
    uint16_t reserved;
    uint32_t mods;
    uint32_t buttons; /* held buttons, injection only */
    double x, y;      /* cursor position (move) */
    double dx, dy;    /* movement or scroll pixels */
} EwEvent;

typedef struct EwScreen {
    double x, y, w, h; /* points, global coordinates, origin top-left */
    const char *name;
} EwScreen;

typedef struct EwCallbacks {
    void *context;
    void (*inject)(void *context, const EwEvent *event);
    void (*control)(void *context, uint8_t op, double x, double y);
    void (*state_changed)(void *context);
    void (*pairing)(void *context, uint8_t kind, const char *peer, const char *detail);
} EwCallbacks;

/* Lifecycle */
EwEngine *ew_engine_start(const char *config_dir, uint8_t role, const char *device_name,
                          const EwScreen *screens, size_t screen_count, EwCallbacks callbacks);
void ew_engine_stop(EwEngine *engine);

/* Inputs */
void ew_engine_set_screens(const EwEngine *engine, const EwScreen *screens, size_t count);
/* Returns 1 when the event must not reach the local system. */
uint8_t ew_engine_local_event(const EwEngine *engine, const EwEvent *event);
/* See ffi.rs for verbs: pair, code, cancel-pairing, forget, place, pause, resume,
 * home, resistance, network-changed. Returns 1 on success. */
uint8_t ew_engine_command(const EwEngine *engine, const char *verb, const char *arg);

/* State as JSON; free with ew_string_free. */
char *ew_engine_snapshot(const EwEngine *engine);
void ew_string_free(char *s);

/* Key and modifier helpers for macOS */
uint16_t ew_key_from_mac(uint16_t mac_virtual_key);
int32_t ew_key_to_mac(uint16_t hid_usage);
uint64_t ew_mac_flags_for_key(uint16_t hid_usage);
uint64_t ew_mac_flags_from_mods(uint32_t mods);

const char *ew_version(void);

#ifdef __cplusplus
}
#endif

#endif /* EDGEWAY_H */
