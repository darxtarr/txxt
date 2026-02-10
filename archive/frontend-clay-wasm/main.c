#define CLAY_IMPLEMENTATION
#include "clay.h"

// Provide strlen for -nostdlib builds
unsigned long strlen(const char* str) {
    unsigned long len = 0;
    while (str && str[len]) len++;
    return len;
}

// Helper to get string length
static inline uint32_t str_len(const char* str) {
    uint32_t len = 0;
    while (str && str[len]) len++;
    return len;
}

// Create Clay_String from char pointer
static inline Clay_String make_string(const char* str) {
    return (Clay_String){ .length = str_len(str), .chars = str };
}

// Font IDs
const uint32_t FONT_ID_BODY_16 = 0;
const uint32_t FONT_ID_BODY_20 = 1;
const uint32_t FONT_ID_TITLE_24 = 2;
const uint32_t FONT_ID_TITLE_32 = 3;

// Colors
const Clay_Color COLOR_BG = (Clay_Color){245, 245, 250, 255};
const Clay_Color COLOR_WHITE = (Clay_Color){255, 255, 255, 255};
const Clay_Color COLOR_SIDEBAR = (Clay_Color){35, 39, 47, 255};
const Clay_Color COLOR_SIDEBAR_HOVER = (Clay_Color){45, 50, 60, 255};
const Clay_Color COLOR_PRIMARY = (Clay_Color){59, 130, 246, 255};
const Clay_Color COLOR_PRIMARY_HOVER = (Clay_Color){37, 99, 235, 255};
const Clay_Color COLOR_TEXT = (Clay_Color){30, 30, 30, 255};
const Clay_Color COLOR_TEXT_LIGHT = (Clay_Color){100, 100, 100, 255};
const Clay_Color COLOR_TEXT_WHITE = (Clay_Color){255, 255, 255, 255};
const Clay_Color COLOR_BORDER = (Clay_Color){220, 220, 230, 255};

// Priority colors
const Clay_Color COLOR_PRIORITY_LOW = (Clay_Color){34, 197, 94, 255};
const Clay_Color COLOR_PRIORITY_MEDIUM = (Clay_Color){234, 179, 8, 255};
const Clay_Color COLOR_PRIORITY_HIGH = (Clay_Color){249, 115, 22, 255};
const Clay_Color COLOR_PRIORITY_URGENT = (Clay_Color){239, 68, 68, 255};

// Status colors
const Clay_Color COLOR_STATUS_PENDING = (Clay_Color){156, 163, 175, 255};
const Clay_Color COLOR_STATUS_INPROGRESS = (Clay_Color){59, 130, 246, 255};
const Clay_Color COLOR_STATUS_COMPLETED = (Clay_Color){34, 197, 94, 255};

// Task Status
typedef enum {
    STATUS_PENDING = 0,
    STATUS_IN_PROGRESS = 1,
    STATUS_COMPLETED = 2
} TaskStatus;

// Priority
typedef enum {
    PRIORITY_LOW = 0,
    PRIORITY_MEDIUM = 1,
    PRIORITY_HIGH = 2,
    PRIORITY_URGENT = 3
} Priority;

// Task data structure
typedef struct {
    // UUID string from backend (36 chars + NUL).
    char id[37];
    // Legacy numeric id (kept only for unused JS interop exports).
    uint32_t legacy_id;
    char title[128];
    char description[512];
    TaskStatus status;
    Priority priority;
    char category[64];
    char service_name[64];
    char due_date[32];
    char assigned_to[64];
    bool selected;
} Task;

typedef struct {
    char id[37];
    char name[64];
} Service;

#define TXXT_MAX_TASKS 100u
#define TXXT_TASK_TITLE_MAX 128u
#define TXXT_TASK_DESC_MAX 512u
#define TXXT_TASK_CATEGORY_MAX 64u
#define TXXT_TASK_DUE_DATE_MAX 32u
#define TXXT_TASK_ASSIGNED_TO_MAX 64u

#define TXXT_TASK_INPUT_HDR_SIZE 16u
#define TXXT_TASK_ID_MAX 37u
// Task input entry layout (bytes):
// 0..3   u32 reserved
// 4..7   u32 status
// 8..11  u32 priority
// 12..48 char id[37]
// 49..51 padding
// 52..179 title[128]
// 180..691 description[512]
// 692..755 category[64]
// 756..819 service_name[64]
// 820..851 due_date[32]
// 852..915 assigned_to[64]
#define TXXT_TASK_INPUT_STRIDE 916u
#define TXXT_TASK_SERVICE_NAME_MAX 64u

#define TXXT_SERVICE_INPUT_HDR_SIZE 16u
#define TXXT_SERVICE_INPUT_STRIDE 128u
#define TXXT_SERVICE_ID_MAX 37u
#define TXXT_SERVICE_NAME_MAX 64u

static uint8_t task_input_buffer[TXXT_TASK_INPUT_HDR_SIZE + (TXXT_MAX_TASKS * TXXT_TASK_INPUT_STRIDE)] = {0};
static uint8_t service_input_buffer[TXXT_SERVICE_INPUT_HDR_SIZE + (64u * TXXT_SERVICE_INPUT_STRIDE)] = {0};

// Filter enum
typedef enum {
    FILTER_ALL = 0,
    FILTER_PENDING = 1,
    FILTER_IN_PROGRESS = 2,
    FILTER_COMPLETED = 3
} FilterStatus;

// App state
typedef struct {
    Task tasks[100];
    uint32_t task_count;
    Service services[64];
    uint32_t service_count;
    char current_user[64];
    int32_t selected_task_index;
    int32_t selected_service_index;
    int32_t pending_create_service_index;
    FilterStatus filter_status;
    bool show_create_modal;
    bool create_panel_visible;
    bool show_detail_panel;
    bool logged_in;
} AppState;

typedef struct {
    float x;
    float y;
    float width;
    float height;
} Rect;

// Global state
AppState app_state = {0};
double window_width = 1024;
double window_height = 768;

static Rect login_rects[2] = {0};

static float data_pulse_remaining = 0.0f;
static float data_pulse_duration = 0.35f;

static double app_time_seconds = 0.0;
static int32_t last_service_click_index = -1;
static double last_service_click_time = 0.0;

// Frame arena for temporary allocations
typedef struct {
    void* memory;
    uintptr_t offset;
} Arena;

Arena frame_arena = {0};

// Static strings for status/priority
static const char* STATUS_STRINGS[] = {"Pending", "In Progress", "Completed"};
static const char* PRIORITY_STRINGS[] = {"Low", "Medium", "High", "Urgent"};

// Helper to get priority color
Clay_Color GetPriorityColor(Priority p) {
    switch (p) {
        case PRIORITY_LOW: return COLOR_PRIORITY_LOW;
        case PRIORITY_MEDIUM: return COLOR_PRIORITY_MEDIUM;
        case PRIORITY_HIGH: return COLOR_PRIORITY_HIGH;
        case PRIORITY_URGENT: return COLOR_PRIORITY_URGENT;
        default: return COLOR_PRIORITY_LOW;
    }
}

static inline uint8_t pulse_alpha(void);
static inline bool string_equals(const char* a, const char* b);
static int32_t find_first_task_for_service(int32_t service_index);

// Helper to get status color
Clay_Color GetStatusColor(TaskStatus s) {
    switch (s) {
        case STATUS_PENDING: return COLOR_STATUS_PENDING;
        case STATUS_IN_PROGRESS: return COLOR_STATUS_INPROGRESS;
        case STATUS_COMPLETED: return COLOR_STATUS_COMPLETED;
        default: return COLOR_STATUS_PENDING;
    }
}

// Custom element data for click handling
typedef struct {
    int32_t task_index;
    int32_t action_type;
    int32_t action_data;
} ClickData;

ClickData* AllocateClickData(ClickData data) {
    ClickData *click_data = (ClickData*)(frame_arena.memory + frame_arena.offset);
    *click_data = data;
    frame_arena.offset += sizeof(ClickData);
    return click_data;
}

// Handle click interaction
void HandleClick(Clay_ElementId elementId, Clay_PointerData pointerInfo, void *userData) {
    ClickData* data = (ClickData*)userData;
    if (pointerInfo.state == CLAY_POINTER_DATA_PRESSED_THIS_FRAME) {
        if (data->action_type == 0) {
            app_state.selected_task_index = data->task_index;
            app_state.show_detail_panel = true;
            app_state.create_panel_visible = false;
        } else if (data->action_type == 1) {
            app_state.show_create_modal = true;
            app_state.create_panel_visible = true;
            app_state.show_detail_panel = false;
        } else if (data->action_type == 2) {
            app_state.show_detail_panel = false;
            app_state.selected_task_index = -1;
        } else if (data->action_type == 3) {
            app_state.filter_status = (FilterStatus)data->action_data;
        } else if (data->action_type == 4) {
            int32_t service_index = data->action_data;
            app_state.selected_service_index = service_index;

            int32_t task_index = find_first_task_for_service(service_index);
            if (task_index >= 0) {
                app_state.selected_task_index = task_index;
                app_state.show_detail_panel = true;
                app_state.create_panel_visible = false;
            } else {
                app_state.selected_task_index = -1;
                app_state.show_detail_panel = false;
            }

            double dt = app_time_seconds - last_service_click_time;
            if (last_service_click_index == service_index && dt <= 0.35) {
                app_state.show_create_modal = true;
                app_state.create_panel_visible = true;
                app_state.show_detail_panel = false;
                app_state.pending_create_service_index = service_index;
            }
            last_service_click_index = service_index;
            last_service_click_time = app_time_seconds;
        }
    }
}

// Sidebar service button
void ServiceButton(const char* label, int32_t service_index, int index) {
    bool is_active = (app_state.selected_service_index == service_index);
    Clay_Color bg_color = is_active ? COLOR_PRIMARY : (Clay_Hovered() ? COLOR_SIDEBAR_HOVER : COLOR_SIDEBAR);

    CLAY(CLAY_IDI("ServiceBtn", index), {
        .layout = {
            .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(40) },
            .padding = { 16, 16, 8, 8 },
            .childAlignment = { .y = CLAY_ALIGN_Y_CENTER }
        },
        .backgroundColor = bg_color,
        .cornerRadius = CLAY_CORNER_RADIUS(6)
    }) {
        Clay_OnHover(HandleClick, AllocateClickData((ClickData){0, 4, service_index}));
        CLAY_TEXT(make_string(label), CLAY_TEXT_CONFIG({
            .fontSize = 14,
            .fontId = FONT_ID_BODY_16,
            .textColor = COLOR_TEXT_WHITE
        }));
    }
}

void StatusFilterButton(const char* label, FilterStatus filter_value, int index) {
    bool is_active = (app_state.filter_status == filter_value);
    Clay_Color bg_color = is_active ? COLOR_PRIMARY : (Clay_Hovered() ? COLOR_PRIMARY_HOVER : COLOR_WHITE);
    Clay_Color text_color = is_active ? COLOR_TEXT_WHITE : COLOR_TEXT;

    CLAY(CLAY_IDI("StatusBtn", index), {
        .layout = {
            .sizing = { CLAY_SIZING_FIT(0), CLAY_SIZING_FIXED(28) },
            .padding = { 10, 10, 4, 4 },
            .childAlignment = { .y = CLAY_ALIGN_Y_CENTER }
        },
        .backgroundColor = bg_color,
        .cornerRadius = CLAY_CORNER_RADIUS(6),
        .border = { .width = { is_active ? 0 : 1, is_active ? 0 : 1, is_active ? 0 : 1, is_active ? 0 : 1 }, .color = COLOR_BORDER }
    }) {
        Clay_OnHover(HandleClick, AllocateClickData((ClickData){0, 3, filter_value}));
        CLAY_TEXT(make_string(label), CLAY_TEXT_CONFIG({
            .fontSize = 12,
            .fontId = FONT_ID_BODY_16,
            .textColor = text_color
        }));
    }
}

// Sidebar component
void Sidebar(void) {
    CLAY(CLAY_ID("Sidebar"), {
        .layout = {
            .sizing = { CLAY_SIZING_FIXED(220), CLAY_SIZING_GROW(0) },
            .layoutDirection = CLAY_TOP_TO_BOTTOM,
            .padding = { 16, 16, 20, 20 },
            .childGap = 8
        },
        .backgroundColor = COLOR_SIDEBAR
    }) {
        // Logo/Title
        CLAY(CLAY_ID("SidebarTitle"), {
            .layout = {
                .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(50) },
                .childAlignment = { .y = CLAY_ALIGN_Y_CENTER }
            }
        }) {
            CLAY_TEXT(CLAY_STRING("Task Tracker"), CLAY_TEXT_CONFIG({
                .fontSize = 24,
                .fontId = FONT_ID_TITLE_24,
                .textColor = COLOR_TEXT_WHITE
            }));
        }

        // Divider
        CLAY(CLAY_ID("SidebarDivider"), {
            .layout = { .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(1) } },
            .backgroundColor = (Clay_Color){60, 65, 75, 255}
        }) {}

        // Spacer
        CLAY(CLAY_ID("SidebarSpacer1"), {
            .layout = { .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(16) } }
        }) {}

        // Services label
        CLAY_TEXT(CLAY_STRING("Services"), CLAY_TEXT_CONFIG({
            .fontSize = 12,
            .fontId = FONT_ID_BODY_16,
            .textColor = (Clay_Color){150, 150, 160, 255}
        }));

        // Spacer
        CLAY(CLAY_ID("SidebarSpacer2"), {
            .layout = { .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(8) } }
        }) {}

        // Service buttons
        ServiceButton("All Services", -1, 0);
        if (app_state.service_count == 0) {
            CLAY_TEXT(CLAY_STRING("No services loaded"), CLAY_TEXT_CONFIG({
                .fontSize = 12,
                .fontId = FONT_ID_BODY_16,
                .textColor = (Clay_Color){170, 170, 180, 255}
            }));
        } else {
            for (uint32_t i = 0; i < app_state.service_count; i++) {
                ServiceButton(app_state.services[i].name, (int32_t)i, (int)(i + 1));
            }
        }

        // Grow spacer
        CLAY(CLAY_ID("SidebarGrowSpacer"), {
            .layout = { .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_GROW(0) } }
        }) {}

        // User info
        if (app_state.logged_in && app_state.current_user[0] != '\0') {
            CLAY(CLAY_ID("UserInfo"), {
                .layout = {
                    .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(50) },
                    .padding = { 12, 12, 8, 8 },
                    .childAlignment = { .y = CLAY_ALIGN_Y_CENTER },
                    .childGap = 8
                },
                .backgroundColor = (Clay_Color){45, 50, 60, 255},
                .cornerRadius = CLAY_CORNER_RADIUS(6)
            }) {
                CLAY(CLAY_ID("UserAvatar"), {
                    .layout = { .sizing = { CLAY_SIZING_FIXED(32), CLAY_SIZING_FIXED(32) } },
                    .backgroundColor = COLOR_PRIMARY,
                    .cornerRadius = CLAY_CORNER_RADIUS(16)
                }) {}

                CLAY_TEXT(make_string(app_state.current_user), CLAY_TEXT_CONFIG({
                    .fontSize = 14,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT_WHITE
                }));
            }
        }
    }
}

// Task card component
void TaskCard(Task* task, int index) {
    bool is_selected = (app_state.selected_task_index == index);
    Clay_Color card_bg = is_selected ? (Clay_Color){235, 245, 255, 255} :
                         (Clay_Hovered() ? (Clay_Color){250, 250, 252, 255} : COLOR_WHITE);
    Clay_Color border_color = is_selected ? COLOR_PRIMARY : COLOR_BORDER;

    CLAY(CLAY_IDI("TaskCard", index), {
        .layout = {
            .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIT(0) },
            .layoutDirection = CLAY_TOP_TO_BOTTOM,
            .padding = { 16, 16, 14, 14 },
            .childGap = 8
        },
        .backgroundColor = card_bg,
        .cornerRadius = CLAY_CORNER_RADIUS(8),
        .border = { .width = { 1, 1, 1, 1 }, .color = border_color }
    }) {
        Clay_OnHover(HandleClick, AllocateClickData((ClickData){index, 0, 0}));

        // Top row: Title + Priority
        CLAY(CLAY_IDI("TaskCardTop", index), {
            .layout = {
                .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIT(0) },
                .childGap = 8,
                .childAlignment = { .y = CLAY_ALIGN_Y_CENTER }
            }
        }) {
            // Priority indicator
            CLAY(CLAY_IDI("PriorityDot", index), {
                .layout = { .sizing = { CLAY_SIZING_FIXED(8), CLAY_SIZING_FIXED(8) } },
                .backgroundColor = GetPriorityColor(task->priority),
                .cornerRadius = CLAY_CORNER_RADIUS(4)
            }) {}

            // Title
            CLAY_TEXT(make_string(task->title), CLAY_TEXT_CONFIG({
                .fontSize = 16,
                .fontId = FONT_ID_BODY_20,
                .textColor = COLOR_TEXT
            }));
        }

        // Description preview
        if (task->description[0] != '\0') {
            CLAY_TEXT(make_string(task->description), CLAY_TEXT_CONFIG({
                .fontSize = 14,
                .fontId = FONT_ID_BODY_16,
                .textColor = COLOR_TEXT_LIGHT
            }));
        }

        // Bottom row: Status + Due date
        CLAY(CLAY_IDI("TaskCardBottom", index), {
            .layout = {
                .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIT(0) },
                .childGap = 8,
                .childAlignment = { .y = CLAY_ALIGN_Y_CENTER }
            }
        }) {
            // Status badge
            CLAY(CLAY_IDI("StatusBadge", index), {
                .layout = {
                    .sizing = { CLAY_SIZING_FIT(0), CLAY_SIZING_FIT(0) },
                    .padding = { 8, 8, 4, 4 }
                },
                .backgroundColor = GetStatusColor(task->status),
                .cornerRadius = CLAY_CORNER_RADIUS(4)
            }) {
                CLAY_TEXT(make_string(STATUS_STRINGS[task->status]), CLAY_TEXT_CONFIG({
                    .fontSize = 12,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT_WHITE
                }));
            }

            // Spacer
            CLAY(CLAY_IDI("TaskCardSpacer", index), {
                .layout = { .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(1) } }
            }) {}

            // Due date
            if (task->due_date[0] != '\0') {
                CLAY_TEXT(make_string(task->due_date), CLAY_TEXT_CONFIG({
                    .fontSize = 12,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT_LIGHT
                }));
            }
        }
    }
}

// Task list component
void TaskList(void) {
    CLAY(CLAY_ID("TaskListContainer"), {
        .layout = {
            .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_GROW(0) },
            .layoutDirection = CLAY_TOP_TO_BOTTOM,
            .padding = { 24, 24, 24, 24 },
            .childGap = 16
        },
        .backgroundColor = COLOR_BG
    }) {
        // Header
        CLAY(CLAY_ID("TaskListHeader"), {
            .layout = {
                .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIT(0) },
                .childAlignment = { .y = CLAY_ALIGN_Y_CENTER },
                .childGap = 16
            }
        }) {
            CLAY_TEXT(CLAY_STRING("Tasks"), CLAY_TEXT_CONFIG({
                .fontSize = 28,
                .fontId = FONT_ID_TITLE_32,
                .textColor = COLOR_TEXT
            }));

            const char* service_label = "All Services";
            if (app_state.selected_service_index >= 0 && app_state.selected_service_index < (int32_t)app_state.service_count) {
                service_label = app_state.services[app_state.selected_service_index].name;
            }

            CLAY(CLAY_ID("TaskListServiceTag"), {
                .layout = {
                    .sizing = { CLAY_SIZING_FIT(0), CLAY_SIZING_FIXED(28) },
                    .padding = { 10, 10, 4, 4 },
                    .childAlignment = { .y = CLAY_ALIGN_Y_CENTER }
                },
                .backgroundColor = (Clay_Color){235, 235, 242, 255},
                .cornerRadius = CLAY_CORNER_RADIUS(6)
            }) {
                CLAY_TEXT(make_string(service_label), CLAY_TEXT_CONFIG({
                    .fontSize = 12,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT
                }));
            }

            // Spacer
            CLAY(CLAY_ID("HeaderSpacer"), {
                .layout = { .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(1) } }
            }) {}

            // Create button
            CLAY(CLAY_ID("CreateBtn"), {
                .layout = {
                    .sizing = { CLAY_SIZING_FIT(0), CLAY_SIZING_FIXED(40) },
                    .padding = { 16, 16, 8, 8 },
                    .childAlignment = { .y = CLAY_ALIGN_Y_CENTER },
                    .childGap = 8
                },
                .backgroundColor = Clay_Hovered() ? COLOR_PRIMARY_HOVER : COLOR_PRIMARY,
                .cornerRadius = CLAY_CORNER_RADIUS(6)
            }) {
                Clay_OnHover(HandleClick, AllocateClickData((ClickData){0, 1, 0}));
                CLAY_TEXT(CLAY_STRING("+ New Task"), CLAY_TEXT_CONFIG({
                    .fontSize = 14,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT_WHITE
                }));
            }
        }

        // Status filters
        CLAY(CLAY_ID("StatusFilters"), {
            .layout = {
                .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIT(0) },
                .childGap = 8,
                .childAlignment = { .y = CLAY_ALIGN_Y_CENTER }
            }
        }) {
            StatusFilterButton("All", FILTER_ALL, 0);
            StatusFilterButton("Pending", FILTER_PENDING, 1);
            StatusFilterButton("In Progress", FILTER_IN_PROGRESS, 2);
            StatusFilterButton("Completed", FILTER_COMPLETED, 3);
        }

        // Task count info
        if (data_pulse_remaining > 0.0f) {
            CLAY(CLAY_ID("TaskListPulse"), {
                .layout = { .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(4) } },
                .backgroundColor = (Clay_Color){59, 130, 246, pulse_alpha()},
                .cornerRadius = CLAY_CORNER_RADIUS(3)
            }) {}
        }

        CLAY_TEXT(CLAY_STRING("Click a task to view details"), CLAY_TEXT_CONFIG({
            .fontSize = 14,
            .fontId = FONT_ID_BODY_16,
            .textColor = COLOR_TEXT_LIGHT
        }));

        // Scrollable task list
        CLAY(CLAY_ID("TaskScroll"), {
            .layout = {
                .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_GROW(0) },
                .layoutDirection = CLAY_TOP_TO_BOTTOM,
                .childGap = 12
            },
            .clip = { .vertical = true, .childOffset = Clay_GetScrollOffset() }
        }) {
            int shown = 0;
            for (uint32_t i = 0; i < app_state.task_count; i++) {
                Task* task = &app_state.tasks[i];

                // Apply filter
                bool show = false;
                switch (app_state.filter_status) {
                    case FILTER_ALL: show = true; break;
                    case FILTER_PENDING: show = (task->status == STATUS_PENDING); break;
                    case FILTER_IN_PROGRESS: show = (task->status == STATUS_IN_PROGRESS); break;
                    case FILTER_COMPLETED: show = (task->status == STATUS_COMPLETED); break;
                }

                if (show && app_state.selected_service_index >= 0 &&
                    app_state.selected_service_index < (int32_t)app_state.service_count) {
                    const char* selected_name = app_state.services[app_state.selected_service_index].name;
                    show = string_equals(task->service_name, selected_name);
                }

                if (show) {
                    TaskCard(task, i);
                    shown++;
                }
            }

            // Empty state
            if (shown == 0) {
                CLAY(CLAY_ID("EmptyState"), {
                    .layout = {
                        .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(200) },
                        .childAlignment = { CLAY_ALIGN_X_CENTER, CLAY_ALIGN_Y_CENTER }
                    }
                }) {
                    CLAY_TEXT(CLAY_STRING("No tasks found. Create one!"), CLAY_TEXT_CONFIG({
                        .fontSize = 16,
                        .fontId = FONT_ID_BODY_16,
                        .textColor = COLOR_TEXT_LIGHT
                    }));
                }
            }
        }
    }
}

// Docked panel (details or create)
void DockPanel(float height) {
    bool show_create = app_state.create_panel_visible;
    bool show_detail = app_state.show_detail_panel && app_state.selected_task_index >= 0 &&
        app_state.selected_task_index < (int32_t)app_state.task_count;

    if (height <= 0.0f || (!show_create && !show_detail)) {
        return;
    }

    Task* task = show_detail ? &app_state.tasks[app_state.selected_task_index] : 0;

    CLAY(CLAY_ID("DockPanel"), {
        .layout = {
            .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(height) },
            .layoutDirection = CLAY_TOP_TO_BOTTOM,
            .padding = { 20, 24, 20, 24 },
            .childGap = 12
        },
        .backgroundColor = COLOR_WHITE,
        .border = { .width = { 1, 0, 0, 0 }, .color = COLOR_BORDER }
    }) {
        // Header
        CLAY(CLAY_ID("DockHeader"), {
            .layout = {
                .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIT(0) },
                .childAlignment = { .y = CLAY_ALIGN_Y_CENTER }
            }
        }) {
            CLAY_TEXT(make_string(show_create ? "Create Task" : "Task Details"), CLAY_TEXT_CONFIG({
                .fontSize = 18,
                .fontId = FONT_ID_TITLE_24,
                .textColor = COLOR_TEXT
            }));

            CLAY(CLAY_ID("DockHeaderSpacer"), {
                .layout = { .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(1) } }
            }) {}

            if (!show_create) {
                CLAY(CLAY_ID("DockCloseBtn"), {
                    .layout = {
                        .sizing = { CLAY_SIZING_FIXED(28), CLAY_SIZING_FIXED(28) },
                        .childAlignment = { CLAY_ALIGN_X_CENTER, CLAY_ALIGN_Y_CENTER }
                    },
                    .backgroundColor = Clay_Hovered() ? (Clay_Color){240, 240, 245, 255} : COLOR_WHITE,
                    .cornerRadius = CLAY_CORNER_RADIUS(4)
                }) {
                    Clay_OnHover(HandleClick, AllocateClickData((ClickData){0, 2, 0}));
                    CLAY_TEXT(CLAY_STRING("X"), CLAY_TEXT_CONFIG({
                        .fontSize = 14,
                        .fontId = FONT_ID_BODY_16,
                        .textColor = COLOR_TEXT_LIGHT
                    }));
                }
            }
        }

        if (show_create) {
            CLAY_TEXT(CLAY_STRING("Fill in the form below. This panel stays docked so you can keep referencing the list."), CLAY_TEXT_CONFIG({
                .fontSize = 13,
                .fontId = FONT_ID_BODY_16,
                .textColor = COLOR_TEXT_LIGHT
            }));
        }

        if (show_detail && task) {
            // Title
            CLAY(CLAY_ID("DockTitle"), {
                .layout = {
                    .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIT(0) },
                    .layoutDirection = CLAY_TOP_TO_BOTTOM,
                    .childGap = 4
                }
            }) {
                CLAY_TEXT(CLAY_STRING("Title"), CLAY_TEXT_CONFIG({
                    .fontSize = 12,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT_LIGHT
                }));
                CLAY_TEXT(make_string(task->title), CLAY_TEXT_CONFIG({
                    .fontSize = 18,
                    .fontId = FONT_ID_BODY_20,
                    .textColor = COLOR_TEXT
                }));
            }

            // Description
            CLAY(CLAY_ID("DockDesc"), {
                .layout = {
                    .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIT(0) },
                    .layoutDirection = CLAY_TOP_TO_BOTTOM,
                    .childGap = 4
                }
            }) {
                CLAY_TEXT(CLAY_STRING("Description"), CLAY_TEXT_CONFIG({
                    .fontSize = 12,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT_LIGHT
                }));
                CLAY_TEXT(make_string(task->description[0] ? task->description : "No description"), CLAY_TEXT_CONFIG({
                    .fontSize = 14,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT
                }));
            }

            // Status
            CLAY(CLAY_ID("DockStatus"), {
                .layout = {
                    .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIT(0) },
                    .layoutDirection = CLAY_TOP_TO_BOTTOM,
                    .childGap = 4
                }
            }) {
                CLAY_TEXT(CLAY_STRING("Status"), CLAY_TEXT_CONFIG({
                    .fontSize = 12,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT_LIGHT
                }));
                CLAY(CLAY_ID("DockStatusBadge"), {
                    .layout = {
                        .sizing = { CLAY_SIZING_FIT(0), CLAY_SIZING_FIT(0) },
                        .padding = { 10, 10, 6, 6 }
                    },
                    .backgroundColor = GetStatusColor(task->status),
                    .cornerRadius = CLAY_CORNER_RADIUS(4)
                }) {
                    CLAY_TEXT(make_string(STATUS_STRINGS[task->status]), CLAY_TEXT_CONFIG({
                        .fontSize = 14,
                        .fontId = FONT_ID_BODY_16,
                        .textColor = COLOR_TEXT_WHITE
                    }));
                }
            }

            // Priority
            CLAY(CLAY_ID("DockPriority"), {
                .layout = {
                    .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIT(0) },
                    .layoutDirection = CLAY_TOP_TO_BOTTOM,
                    .childGap = 4
                }
            }) {
                CLAY_TEXT(CLAY_STRING("Priority"), CLAY_TEXT_CONFIG({
                    .fontSize = 12,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT_LIGHT
                }));
                CLAY(CLAY_ID("DockPriorityRow"), {
                    .layout = {
                        .sizing = { CLAY_SIZING_FIT(0), CLAY_SIZING_FIT(0) },
                        .childGap = 8,
                        .childAlignment = { .y = CLAY_ALIGN_Y_CENTER }
                    }
                }) {
                    CLAY(CLAY_ID("DockPriorityDot"), {
                        .layout = { .sizing = { CLAY_SIZING_FIXED(10), CLAY_SIZING_FIXED(10) } },
                        .backgroundColor = GetPriorityColor(task->priority),
                        .cornerRadius = CLAY_CORNER_RADIUS(5)
                    }) {}
                    CLAY_TEXT(make_string(PRIORITY_STRINGS[task->priority]), CLAY_TEXT_CONFIG({
                        .fontSize = 14,
                        .fontId = FONT_ID_BODY_16,
                        .textColor = COLOR_TEXT
                    }));
                }
            }

            if (task->service_name[0] != '\0') {
                CLAY(CLAY_ID("DockService"), {
                    .layout = {
                        .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIT(0) },
                        .layoutDirection = CLAY_TOP_TO_BOTTOM,
                        .childGap = 4
                    }
                }) {
                    CLAY_TEXT(CLAY_STRING("Service"), CLAY_TEXT_CONFIG({
                        .fontSize = 12,
                        .fontId = FONT_ID_BODY_16,
                        .textColor = COLOR_TEXT_LIGHT
                    }));
                    CLAY_TEXT(make_string(task->service_name), CLAY_TEXT_CONFIG({
                        .fontSize = 14,
                        .fontId = FONT_ID_BODY_16,
                        .textColor = COLOR_TEXT
                    }));
                }
            }

            if (task->due_date[0] != '\0') {
                CLAY(CLAY_ID("DockDue"), {
                    .layout = {
                        .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIT(0) },
                        .layoutDirection = CLAY_TOP_TO_BOTTOM,
                        .childGap = 4
                    }
                }) {
                    CLAY_TEXT(CLAY_STRING("Due Date"), CLAY_TEXT_CONFIG({
                        .fontSize = 12,
                        .fontId = FONT_ID_BODY_16,
                        .textColor = COLOR_TEXT_LIGHT
                    }));
                    CLAY_TEXT(make_string(task->due_date), CLAY_TEXT_CONFIG({
                        .fontSize = 14,
                        .fontId = FONT_ID_BODY_16,
                        .textColor = COLOR_TEXT
                    }));
                }
            }

            if (task->assigned_to[0] != '\0') {
                CLAY(CLAY_ID("DockAssigned"), {
                    .layout = {
                        .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIT(0) },
                        .layoutDirection = CLAY_TOP_TO_BOTTOM,
                        .childGap = 4
                    }
                }) {
                    CLAY_TEXT(CLAY_STRING("Assigned To"), CLAY_TEXT_CONFIG({
                        .fontSize = 12,
                        .fontId = FONT_ID_BODY_16,
                        .textColor = COLOR_TEXT_LIGHT
                    }));
                    CLAY_TEXT(make_string(task->assigned_to), CLAY_TEXT_CONFIG({
                        .fontSize = 14,
                        .fontId = FONT_ID_BODY_16,
                        .textColor = COLOR_TEXT
                    }));
                }
            }
        }
    }
}

// Login screen
void LoginScreen(void) {
    CLAY(CLAY_ID("LoginOuter"), {
        .layout = {
            .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_GROW(0) },
            .childAlignment = { CLAY_ALIGN_X_CENTER, CLAY_ALIGN_Y_CENTER }
        },
        .backgroundColor = COLOR_BG
    }) {
        CLAY(CLAY_ID("LoginBox"), {
            .layout = {
                .sizing = { CLAY_SIZING_FIXED(400), CLAY_SIZING_FIT(0) },
                .layoutDirection = CLAY_TOP_TO_BOTTOM,
                .padding = { 32, 32, 32, 32 },
                .childGap = 24,
                .childAlignment = { .x = CLAY_ALIGN_X_CENTER }
            },
            .backgroundColor = COLOR_WHITE,
            .cornerRadius = CLAY_CORNER_RADIUS(12),
            .border = { .width = { 1, 1, 1, 1 }, .color = COLOR_BORDER }
        }) {
            CLAY_TEXT(CLAY_STRING("Task Tracker"), CLAY_TEXT_CONFIG({
                .fontSize = 32,
                .fontId = FONT_ID_TITLE_32,
                .textColor = COLOR_TEXT
            }));

            CLAY_TEXT(CLAY_STRING("Sign in to continue"), CLAY_TEXT_CONFIG({
                .fontSize = 14,
                .fontId = FONT_ID_BODY_16,
                .textColor = COLOR_TEXT_LIGHT
            }));

            // Username input placeholder
            CLAY(CLAY_ID("UsernameInput"), {
                .layout = {
                    .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(44) },
                    .padding = { 12, 12, 10, 10 }
                },
                .backgroundColor = (Clay_Color){250, 250, 252, 255},
                .cornerRadius = CLAY_CORNER_RADIUS(6),
                .border = { .width = { 1, 1, 1, 1 }, .color = COLOR_BORDER }
            }) {
                CLAY_TEXT(CLAY_STRING("Username"), CLAY_TEXT_CONFIG({
                    .fontSize = 14,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT_LIGHT
                }));
            }

            // Password input placeholder
            CLAY(CLAY_ID("PasswordInput"), {
                .layout = {
                    .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(44) },
                    .padding = { 12, 12, 10, 10 }
                },
                .backgroundColor = (Clay_Color){250, 250, 252, 255},
                .cornerRadius = CLAY_CORNER_RADIUS(6),
                .border = { .width = { 1, 1, 1, 1 }, .color = COLOR_BORDER }
            }) {
                CLAY_TEXT(CLAY_STRING("Password"), CLAY_TEXT_CONFIG({
                    .fontSize = 14,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT_LIGHT
                }));
            }

            // Login button
            CLAY(CLAY_ID("LoginBtn"), {
                .layout = {
                    .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(44) },
                    .childAlignment = { CLAY_ALIGN_X_CENTER, CLAY_ALIGN_Y_CENTER }
                },
                .backgroundColor = Clay_Hovered() ? COLOR_PRIMARY_HOVER : COLOR_PRIMARY,
                .cornerRadius = CLAY_CORNER_RADIUS(6)
            }) {
                CLAY_TEXT(CLAY_STRING("Sign In"), CLAY_TEXT_CONFIG({
                    .fontSize = 16,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT_WHITE
                }));
            }
        }
    }
}

// Main app layout
void MainLayout(void) {
    float dock_height = (app_state.create_panel_visible || app_state.show_detail_panel) ? (float)(window_height * 0.33f) : 0.0f;

    CLAY(CLAY_ID("MainContainer"), {
        .layout = {
            .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_GROW(0) },
            .layoutDirection = CLAY_LEFT_TO_RIGHT
        }
    }) {
        Sidebar();
        CLAY(CLAY_ID("MainColumn"), {
            .layout = {
                .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_GROW(0) },
                .layoutDirection = CLAY_TOP_TO_BOTTOM
            }
        }) {
            TaskList();
            DockPanel(dock_height);
        }
    }
}

// Create the layout
Clay_RenderCommandArray CreateLayout(void) {
    Clay_BeginLayout();

    CLAY(CLAY_ID("Root"), {
        .layout = {
            .sizing = { CLAY_SIZING_FIXED(window_width), CLAY_SIZING_FIXED(window_height) }
        }
    }) {
        if (app_state.logged_in) {
            MainLayout();
        } else {
            LoginScreen();
        }
    }

    return Clay_EndLayout();
}

static void UpdateLoginRects(void) {
    Clay_ElementData u = Clay_GetElementData(Clay_GetElementId(CLAY_STRING("UsernameInput")));
    Clay_ElementData p = Clay_GetElementData(Clay_GetElementId(CLAY_STRING("PasswordInput")));

    if (u.found) {
        login_rects[0] = (Rect){
            .x = u.boundingBox.x,
            .y = u.boundingBox.y,
            .width = u.boundingBox.width,
            .height = u.boundingBox.height,
        };
    }
    else {
        login_rects[0] = (Rect){ .x = -1, .y = -1, .width = 0, .height = 0 };
    }

    if (p.found) {
        login_rects[1] = (Rect){
            .x = p.boundingBox.x,
            .y = p.boundingBox.y,
            .width = p.boundingBox.width,
            .height = p.boundingBox.height,
        };
    }
    else {
        login_rects[1] = (Rect){ .x = -1, .y = -1, .width = 0, .height = 0 };
    }
}

static inline uint32_t read_u32_le(const uint8_t* p) {
    return (uint32_t)p[0] |
           ((uint32_t)p[1] << 8) |
           ((uint32_t)p[2] << 16) |
           ((uint32_t)p[3] << 24);
}

static void copy_fixed_string(char* dst, uint32_t dst_cap, const uint8_t* src, uint32_t src_cap) {
    if (!dst || dst_cap == 0) {
        return;
    }
    uint32_t i = 0;
    for (; i + 1 < dst_cap && i < src_cap; i++) {
        uint8_t c = src[i];
        if (c == 0) {
            break;
        }
        dst[i] = (char)c;
    }
    dst[i] = '\0';
}

static inline bool string_equals(const char* a, const char* b) {
    if (a == b) {
        return true;
    }
    if (!a || !b) {
        return false;
    }
    uint32_t i = 0;
    while (a[i] && b[i]) {
        if (a[i] != b[i]) {
            return false;
        }
        i++;
    }
    return a[i] == b[i];
}

static int32_t find_first_task_for_service(int32_t service_index) {
    if (service_index < 0 || service_index >= (int32_t)app_state.service_count) {
        return -1;
    }

    const char* name = app_state.services[service_index].name;
    for (uint32_t i = 0; i < app_state.task_count; i++) {
        if (string_equals(app_state.tasks[i].service_name, name)) {
            return (int32_t)i;
        }
    }
    return -1;
}

static inline uint8_t pulse_alpha(void) {
    if (data_pulse_remaining <= 0.0f || data_pulse_duration <= 0.0f) {
        return 0;
    }
    float t = data_pulse_remaining / data_pulse_duration;
    if (t < 0.0f) t = 0.0f;
    if (t > 1.0f) t = 1.0f;
    float a = 30.0f + (1.0f - t) * 140.0f;
    if (a < 0.0f) a = 0.0f;
    if (a > 255.0f) a = 255.0f;
    return (uint8_t)a;
}

// WASM exports
CLAY_WASM_EXPORT("SetScratchMemory") void SetScratchMemory(void* memory) {
    frame_arena.memory = memory;
}

CLAY_WASM_EXPORT("GetTaskInputBuffer") uint32_t GetTaskInputBuffer(void) {
    return (uint32_t)(uintptr_t)task_input_buffer;
}

CLAY_WASM_EXPORT("GetServiceInputBuffer") uint32_t GetServiceInputBuffer(void) {
    return (uint32_t)(uintptr_t)service_input_buffer;
}

CLAY_WASM_EXPORT("ApplyTaskInputBuffer") void ApplyTaskInputBuffer(uint32_t count) {
    uint32_t max = count;
    if (max > TXXT_MAX_TASKS) {
        max = TXXT_MAX_TASKS;
    }

    for (uint32_t i = 0; i < max; i++) {
        Task* task = &app_state.tasks[i];
        const uint8_t* entry = task_input_buffer + TXXT_TASK_INPUT_HDR_SIZE + (i * TXXT_TASK_INPUT_STRIDE);

        task->legacy_id = read_u32_le(entry + 0);
        task->status = (TaskStatus)read_u32_le(entry + 4);
        task->priority = (Priority)read_u32_le(entry + 8);

        copy_fixed_string(task->id, sizeof(task->id), entry + 12, TXXT_TASK_ID_MAX);
        copy_fixed_string(task->title, sizeof(task->title), entry + 52, TXXT_TASK_TITLE_MAX);
        copy_fixed_string(task->description, sizeof(task->description), entry + 180, TXXT_TASK_DESC_MAX);
        copy_fixed_string(task->category, sizeof(task->category), entry + 692, TXXT_TASK_CATEGORY_MAX);
        copy_fixed_string(task->service_name, sizeof(task->service_name), entry + 756, TXXT_TASK_SERVICE_NAME_MAX);
        copy_fixed_string(task->due_date, sizeof(task->due_date), entry + 820, TXXT_TASK_DUE_DATE_MAX);
        copy_fixed_string(task->assigned_to, sizeof(task->assigned_to), entry + 852, TXXT_TASK_ASSIGNED_TO_MAX);

        task->selected = false;
    }

    app_state.task_count = max;
    if (app_state.selected_task_index >= (int32_t)max) {
        app_state.selected_task_index = -1;
        app_state.show_detail_panel = false;
    }
}

CLAY_WASM_EXPORT("ApplyServiceInputBuffer") void ApplyServiceInputBuffer(uint32_t count) {
    uint32_t max = count;
    if (max > 64u) {
        max = 64u;
    }

    for (uint32_t i = 0; i < max; i++) {
        Service* service = &app_state.services[i];
        const uint8_t* entry = service_input_buffer + TXXT_SERVICE_INPUT_HDR_SIZE + (i * TXXT_SERVICE_INPUT_STRIDE);

        copy_fixed_string(service->id, sizeof(service->id), entry + 0, TXXT_SERVICE_ID_MAX);
        copy_fixed_string(service->name, sizeof(service->name), entry + 64, TXXT_SERVICE_NAME_MAX);
    }

    app_state.service_count = max;
    if (app_state.selected_service_index >= (int32_t)max) {
        app_state.selected_service_index = -1;
    }
}

CLAY_WASM_EXPORT("GetCurrentUserBuffer") uint32_t GetCurrentUserBuffer(void) {
    return (uint32_t)(uintptr_t)app_state.current_user;
}

CLAY_WASM_EXPORT("SetDataDirtyPulse") void SetDataDirtyPulse(float seconds) {
    float duration = seconds > 0.0f ? seconds : 0.35f;
    data_pulse_duration = duration;
    if (data_pulse_remaining < duration) {
        data_pulse_remaining = duration;
    }
}

#define TXXT_PACKED_CMD_SIZE 64u
#define TXXT_PACKED_HDR_SIZE 16u

static inline void write_u16(uint8_t* p, uint16_t v) {
    p[0] = (uint8_t)(v & 0xff);
    p[1] = (uint8_t)((v >> 8) & 0xff);
}

static inline void write_i16(uint8_t* p, int16_t v) {
    write_u16(p, (uint16_t)v);
}

static inline void write_u32(uint8_t* p, uint32_t v) {
    p[0] = (uint8_t)(v & 0xff);
    p[1] = (uint8_t)((v >> 8) & 0xff);
    p[2] = (uint8_t)((v >> 16) & 0xff);
    p[3] = (uint8_t)((v >> 24) & 0xff);
}

static inline void write_f32(uint8_t* p, float v) {
    union { float f; uint32_t u; } u = { .f = v };
    write_u32(p, u.u);
}

static void PackRenderCommands(uint32_t scratch_address, Clay_RenderCommandArray cmds) {
    if (scratch_address == 0) {
        return;
    }

    uint8_t* base = (uint8_t*)(uintptr_t)scratch_address;
    uint32_t len = (uint32_t)cmds.length;

    // Header
    // u32 length
    // u32 command_size
    // u32 commands_ptr
    // u32 reserved
    write_u32(base + 0, len);
    write_u32(base + 4, TXXT_PACKED_CMD_SIZE);
    write_u32(base + 8, scratch_address + TXXT_PACKED_HDR_SIZE);
    write_u32(base + 12, 0);

    uint8_t* out = base + TXXT_PACKED_HDR_SIZE;
    for (uint32_t i = 0; i < len; i++) {
        Clay_RenderCommand* cmd = &cmds.internalArray[i];
        uint8_t* c = out + (i * TXXT_PACKED_CMD_SIZE);

        // Zero the whole command.
        for (uint32_t j = 0; j < TXXT_PACKED_CMD_SIZE; j++) {
            c[j] = 0;
        }

        c[0] = (uint8_t)cmd->commandType;
        c[1] = 0;
        write_i16(c + 2, cmd->zIndex);

        write_f32(c + 4, cmd->boundingBox.x);
        write_f32(c + 8, cmd->boundingBox.y);
        write_f32(c + 12, cmd->boundingBox.width);
        write_f32(c + 16, cmd->boundingBox.height);

        // Payload starts at offset 20.
        switch (cmd->commandType) {
            case CLAY_RENDER_COMMAND_TYPE_RECTANGLE: {
                Clay_RectangleRenderData* r = &cmd->renderData.rectangle;
                write_f32(c + 20, r->backgroundColor.r);
                write_f32(c + 24, r->backgroundColor.g);
                write_f32(c + 28, r->backgroundColor.b);
                write_f32(c + 32, r->backgroundColor.a);

                write_f32(c + 36, r->cornerRadius.topLeft);
                write_f32(c + 40, r->cornerRadius.topRight);
                write_f32(c + 44, r->cornerRadius.bottomRight);
                write_f32(c + 48, r->cornerRadius.bottomLeft);
                break;
            }

            case CLAY_RENDER_COMMAND_TYPE_TEXT: {
                Clay_TextRenderData* t = &cmd->renderData.text;
                // stringContents: length + chars pointer
                write_u32(c + 20, (uint32_t)(uintptr_t)t->stringContents.chars);
                write_u32(c + 24, (uint32_t)t->stringContents.length);
                write_u16(c + 28, t->fontId);
                write_u16(c + 30, t->fontSize);
                write_u16(c + 32, t->letterSpacing);
                write_u16(c + 34, t->lineHeight);

                write_f32(c + 36, t->textColor.r);
                write_f32(c + 40, t->textColor.g);
                write_f32(c + 44, t->textColor.b);
                write_f32(c + 48, t->textColor.a);
                break;
            }

            case CLAY_RENDER_COMMAND_TYPE_BORDER: {
                Clay_BorderRenderData* b = &cmd->renderData.border;
                write_f32(c + 20, b->color.r);
                write_f32(c + 24, b->color.g);
                write_f32(c + 28, b->color.b);
                write_f32(c + 32, b->color.a);

                write_f32(c + 36, b->cornerRadius.topLeft);
                write_f32(c + 40, b->cornerRadius.topRight);
                write_f32(c + 44, b->cornerRadius.bottomRight);
                write_f32(c + 48, b->cornerRadius.bottomLeft);

                write_u16(c + 52, b->width.left);
                write_u16(c + 54, b->width.right);
                write_u16(c + 56, b->width.top);
                write_u16(c + 58, b->width.bottom);
                write_u16(c + 60, b->width.betweenChildren);
                write_u16(c + 62, 0);
                break;
            }

            case CLAY_RENDER_COMMAND_TYPE_IMAGE: {
                Clay_ImageRenderData* im = &cmd->renderData.image;
                write_f32(c + 20, im->backgroundColor.r);
                write_f32(c + 24, im->backgroundColor.g);
                write_f32(c + 28, im->backgroundColor.b);
                write_f32(c + 32, im->backgroundColor.a);

                write_f32(c + 36, im->cornerRadius.topLeft);
                write_f32(c + 40, im->cornerRadius.topRight);
                write_f32(c + 44, im->cornerRadius.bottomRight);
                write_f32(c + 48, im->cornerRadius.bottomLeft);

                write_u32(c + 52, (uint32_t)(uintptr_t)im->imageData);
                break;
            }

            case CLAY_RENDER_COMMAND_TYPE_CUSTOM: {
                Clay_CustomRenderData* cu = &cmd->renderData.custom;
                write_f32(c + 20, cu->backgroundColor.r);
                write_f32(c + 24, cu->backgroundColor.g);
                write_f32(c + 28, cu->backgroundColor.b);
                write_f32(c + 32, cu->backgroundColor.a);

                write_f32(c + 36, cu->cornerRadius.topLeft);
                write_f32(c + 40, cu->cornerRadius.topRight);
                write_f32(c + 44, cu->cornerRadius.bottomRight);
                write_f32(c + 48, cu->cornerRadius.bottomLeft);

                write_u32(c + 52, (uint32_t)(uintptr_t)cu->customData);
                break;
            }

            case CLAY_RENDER_COMMAND_TYPE_SCISSOR_START:
            case CLAY_RENDER_COMMAND_TYPE_SCISSOR_END:
            case CLAY_RENDER_COMMAND_TYPE_NONE:
            default:
                break;
        }
    }
}

CLAY_WASM_EXPORT("UpdateDrawFrame") void UpdateDrawFrame(
    uint32_t cmd_buffer_address,
    float width, float height,
    float mouse_wheel_x, float mouse_wheel_y,
    float mouse_x, float mouse_y,
    bool touch_down, bool mouse_down,
    float delta_time
) {
    frame_arena.offset = 0;
    window_width = width;
    window_height = height;
    app_time_seconds += delta_time;

    if (data_pulse_remaining > 0.0f) {
        data_pulse_remaining -= delta_time;
        if (data_pulse_remaining < 0.0f) {
            data_pulse_remaining = 0.0f;
        }
    }

    Clay_SetLayoutDimensions((Clay_Dimensions){width, height});
    Clay_SetPointerState((Clay_Vector2){mouse_x, mouse_y}, mouse_down || touch_down);
    Clay_UpdateScrollContainers(touch_down, (Clay_Vector2){mouse_wheel_x, mouse_wheel_y}, delta_time);

    Clay_RenderCommandArray cmds = CreateLayout();
    UpdateLoginRects();
    PackRenderCommands(cmd_buffer_address, cmds);
}

// JS interop functions
CLAY_WASM_EXPORT("GetAppState") AppState* GetAppState(void) {
    return &app_state;
}

CLAY_WASM_EXPORT("SetLoggedIn") void SetLoggedIn(bool logged_in) {
    app_state.logged_in = logged_in;
}

CLAY_WASM_EXPORT("AddTask") void AddTask(
    uint32_t id,
    uint32_t status,
    uint32_t priority
) {
    if (app_state.task_count < 100) {
        Task* task = &app_state.tasks[app_state.task_count];
        task->legacy_id = id;
        task->id[0] = 0;
        task->status = (TaskStatus)status;
        task->priority = (Priority)priority;
        app_state.task_count++;
    }
}

CLAY_WASM_EXPORT("ClearTasks") void ClearTasks(void) {
    app_state.task_count = 0;
}

CLAY_WASM_EXPORT("GetTaskCount") uint32_t GetTaskCount(void) {
    return app_state.task_count;
}

CLAY_WASM_EXPORT("GetSelectedTaskIndex") int32_t GetSelectedTaskIndex(void) {
    return app_state.selected_task_index;
}

CLAY_WASM_EXPORT("GetShowCreateModal") bool GetShowCreateModal(void) {
    bool result = app_state.show_create_modal;
    app_state.show_create_modal = false;
    return result;
}

CLAY_WASM_EXPORT("GetPendingCreateServiceIndex") int32_t GetPendingCreateServiceIndex(void) {
    int32_t result = app_state.pending_create_service_index;
    app_state.pending_create_service_index = -1;
    return result;
}

CLAY_WASM_EXPORT("SetCreatePanelVisible") void SetCreatePanelVisible(bool visible) {
    app_state.create_panel_visible = visible;
    if (!visible) {
        app_state.pending_create_service_index = -1;
    }
}

CLAY_WASM_EXPORT("InitApp") void InitApp(void) {
    app_state.logged_in = false;
    app_state.task_count = 0;
    app_state.service_count = 0;
    app_state.selected_task_index = -1;
    app_state.selected_service_index = -1;
    app_state.pending_create_service_index = -1;
    app_state.filter_status = FILTER_ALL;
    app_state.show_create_modal = false;
    app_state.create_panel_visible = false;
    app_state.show_detail_panel = false;
    app_state.current_user[0] = '\0';
}

CLAY_WASM_EXPORT("GetLoginRect") Rect* GetLoginRect(uint32_t which) {
    if (which >= 2) {
        return 0;
    }
    return &login_rects[which];
}

// Dummy main for WASM
int main(void) {
    return 0;
}
