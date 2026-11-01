#pragma once
#include "FreeRTOS.h"
#define BIT0 1u
#define BIT1 2u
typedef uint32_t EventBits_t;
typedef struct { EventBits_t bits; } EventGroup_t;
typedef EventGroup_t *EventGroupHandle_t;
EventGroupHandle_t xEventGroupCreate(void);
void vEventGroupDelete(EventGroupHandle_t);
EventBits_t xEventGroupSetBits(EventGroupHandle_t, EventBits_t);
EventBits_t xEventGroupClearBits(EventGroupHandle_t, EventBits_t);
EventBits_t xEventGroupWaitBits(EventGroupHandle_t, EventBits_t, bool, bool, uint32_t);
