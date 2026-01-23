#define CLAY_IMPLEMENTATION
#include "clay.h"

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

// Task data structure (simplified for display)
typedef struct {
    uint32_t id;
    char title[128];
    char description[512];
    TaskStatus status;
    Priority priority;
    char category[64];
    char due_date[32];
    char assigned_to[64];
    bool selected;
} Task;

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
    char current_user[64];
    int32_t selected_task_index;
    FilterStatus filter_status;
    bool show_create_modal;
    bool show_detail_panel;
    bool logged_in;
    // Input state - managed by JS
    char input_title[256];
    char input_description[2048];
} AppState;

// Global state
AppState app_state = {0};
double window_width = 1024;
double window_height = 768;

// Frame arena for temporary allocations
typedef struct {
    void* memory;
    uintptr_t offset;
} Arena;

Arena frame_arena = {0};

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

// Helper to get status color
Clay_Color GetStatusColor(TaskStatus s) {
    switch (s) {
        case STATUS_PENDING: return COLOR_STATUS_PENDING;
        case STATUS_IN_PROGRESS: return COLOR_STATUS_INPROGRESS;
        case STATUS_COMPLETED: return COLOR_STATUS_COMPLETED;
        default: return COLOR_STATUS_PENDING;
    }
}

const char* GetStatusText(TaskStatus s) {
    switch (s) {
        case STATUS_PENDING: return "Pending";
        case STATUS_IN_PROGRESS: return "In Progress";
        case STATUS_COMPLETED: return "Completed";
        default: return "Unknown";
    }
}

const char* GetPriorityText(Priority p) {
    switch (p) {
        case PRIORITY_LOW: return "Low";
        case PRIORITY_MEDIUM: return "Medium";
        case PRIORITY_HIGH: return "High";
        case PRIORITY_URGENT: return "Urgent";
        default: return "Unknown";
    }
}

// Custom element data for click handling
typedef struct {
    int32_t task_index;
    int32_t action_type; // 0=select, 1=create, 2=close_detail, 3=filter, 4=status_change
    int32_t action_data;
} ClickData;

ClickData* AllocateClickData(ClickData data) {
    ClickData *click_data = (ClickData*)(frame_arena.memory + frame_arena.offset);
    *click_data = data;
    frame_arena.offset += sizeof(ClickData);
    return click_data;
}

// Handle click interaction
void HandleTaskClick(Clay_ElementId elementId, Clay_PointerData pointerInfo, void *userData) {
    ClickData* data = (ClickData*)userData;
    if (pointerInfo.state == CLAY_POINTER_DATA_PRESSED_THIS_FRAME) {
        if (data->action_type == 0) {
            // Select task
            app_state.selected_task_index = data->task_index;
            app_state.show_detail_panel = true;
        } else if (data->action_type == 1) {
            // Open create modal
            app_state.show_create_modal = true;
        } else if (data->action_type == 2) {
            // Close detail panel
            app_state.show_detail_panel = false;
            app_state.selected_task_index = -1;
        } else if (data->action_type == 3) {
            // Change filter
            app_state.filter_status = (FilterStatus)data->action_data;
        }
    }
}

// Sidebar filter button
void FilterButton(const char* label, FilterStatus filter_value, int index) {
    bool is_active = (app_state.filter_status == filter_value);
    Clay_Color bg_color = is_active ? COLOR_PRIMARY : (Clay_Hovered() ? COLOR_SIDEBAR_HOVER : COLOR_SIDEBAR);

    CLAY(CLAY_IDI("FilterBtn", index), {
        .layout = {
            .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(40) },
            .padding = { 16, 16, 8, 8 },
            .childAlignment = { .y = CLAY_ALIGN_Y_CENTER }
        },
        .backgroundColor = bg_color,
        .cornerRadius = CLAY_CORNER_RADIUS(6)
    }) {
        Clay_OnHover(HandleTaskClick, AllocateClickData((ClickData){0, 3, filter_value}));
        CLAY_TEXT(CLAY_STRING(label), CLAY_TEXT_CONFIG({
            .fontSize = 14,
            .fontId = FONT_ID_BODY_16,
            .textColor = COLOR_TEXT_WHITE
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

        // Filter label
        CLAY_TEXT(CLAY_STRING("Status Filter"), CLAY_TEXT_CONFIG({
            .fontSize = 12,
            .fontId = FONT_ID_BODY_16,
            .textColor = (Clay_Color){150, 150, 160, 255}
        }));

        // Spacer
        CLAY(CLAY_ID("SidebarSpacer2"), {
            .layout = { .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(8) } }
        }) {}

        // Filter buttons
        FilterButton("All Tasks", FILTER_ALL, 0);
        FilterButton("Pending", FILTER_PENDING, 1);
        FilterButton("In Progress", FILTER_IN_PROGRESS, 2);
        FilterButton("Completed", FILTER_COMPLETED, 3);

        // Grow spacer
        CLAY(CLAY_ID("SidebarGrowSpacer"), {
            .layout = { .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_GROW(0) } }
        }) {}

        // User info
        if (app_state.logged_in) {
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
                // User avatar placeholder
                CLAY(CLAY_ID("UserAvatar"), {
                    .layout = { .sizing = { CLAY_SIZING_FIXED(32), CLAY_SIZING_FIXED(32) } },
                    .backgroundColor = COLOR_PRIMARY,
                    .cornerRadius = CLAY_CORNER_RADIUS(16)
                }) {}

                CLAY_TEXT(CLAY_STRING(app_state.current_user), CLAY_TEXT_CONFIG({
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
        Clay_OnHover(HandleTaskClick, AllocateClickData((ClickData){index, 0, 0}));

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
            CLAY_TEXT(CLAY_STRING(task->title), CLAY_TEXT_CONFIG({
                .fontSize = 16,
                .fontId = FONT_ID_BODY_20,
                .textColor = COLOR_TEXT
            }));
        }

        // Description preview
        if (task->description[0] != '\0') {
            CLAY_TEXT(CLAY_STRING(task->description), CLAY_TEXT_CONFIG({
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
                CLAY_TEXT(CLAY_STRING(GetStatusText(task->status)), CLAY_TEXT_CONFIG({
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
                CLAY_TEXT(CLAY_STRING(task->due_date), CLAY_TEXT_CONFIG({
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
                Clay_OnHover(HandleTaskClick, AllocateClickData((ClickData){0, 1, 0}));
                CLAY_TEXT(CLAY_STRING("+ New Task"), CLAY_TEXT_CONFIG({
                    .fontSize = 14,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT_WHITE
                }));
            }
        }

        // Task count
        CLAY_TEXT(CLAY_STRING("Showing all tasks"), CLAY_TEXT_CONFIG({
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
            // Render filtered tasks
            for (uint32_t i = 0; i < app_state.task_count; i++) {
                Task* task = &app_state.tasks[i];

                // Apply filter
                bool show = false;
                switch (app_state.filter_status) {
                    case FILTER_ALL:
                        show = true;
                        break;
                    case FILTER_PENDING:
                        show = (task->status == STATUS_PENDING);
                        break;
                    case FILTER_IN_PROGRESS:
                        show = (task->status == STATUS_IN_PROGRESS);
                        break;
                    case FILTER_COMPLETED:
                        show = (task->status == STATUS_COMPLETED);
                        break;
                }

                if (show) {
                    TaskCard(task, i);
                }
            }

            // Empty state
            if (app_state.task_count == 0) {
                CLAY(CLAY_ID("EmptyState"), {
                    .layout = {
                        .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(200) },
                        .childAlignment = { CLAY_ALIGN_X_CENTER, CLAY_ALIGN_Y_CENTER }
                    }
                }) {
                    CLAY_TEXT(CLAY_STRING("No tasks yet. Create one!"), CLAY_TEXT_CONFIG({
                        .fontSize = 16,
                        .fontId = FONT_ID_BODY_16,
                        .textColor = COLOR_TEXT_LIGHT
                    }));
                }
            }
        }
    }
}

// Detail panel
void DetailPanel(void) {
    if (!app_state.show_detail_panel || app_state.selected_task_index < 0) {
        return;
    }

    Task* task = &app_state.tasks[app_state.selected_task_index];

    CLAY(CLAY_ID("DetailPanel"), {
        .layout = {
            .sizing = { CLAY_SIZING_FIXED(350), CLAY_SIZING_GROW(0) },
            .layoutDirection = CLAY_TOP_TO_BOTTOM,
            .padding = { 24, 24, 24, 24 },
            .childGap = 16
        },
        .backgroundColor = COLOR_WHITE,
        .border = { .width = { .left = 1 }, .color = COLOR_BORDER }
    }) {
        // Header
        CLAY(CLAY_ID("DetailHeader"), {
            .layout = {
                .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIT(0) },
                .childAlignment = { .y = CLAY_ALIGN_Y_CENTER }
            }
        }) {
            CLAY_TEXT(CLAY_STRING("Task Details"), CLAY_TEXT_CONFIG({
                .fontSize = 20,
                .fontId = FONT_ID_TITLE_24,
                .textColor = COLOR_TEXT
            }));

            // Spacer
            CLAY(CLAY_ID("DetailHeaderSpacer"), {
                .layout = { .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(1) } }
            }) {}

            // Close button
            CLAY(CLAY_ID("CloseBtn"), {
                .layout = {
                    .sizing = { CLAY_SIZING_FIXED(32), CLAY_SIZING_FIXED(32) },
                    .childAlignment = { CLAY_ALIGN_X_CENTER, CLAY_ALIGN_Y_CENTER }
                },
                .backgroundColor = Clay_Hovered() ? (Clay_Color){240, 240, 245, 255} : COLOR_WHITE,
                .cornerRadius = CLAY_CORNER_RADIUS(4)
            }) {
                Clay_OnHover(HandleTaskClick, AllocateClickData((ClickData){0, 2, 0}));
                CLAY_TEXT(CLAY_STRING("X"), CLAY_TEXT_CONFIG({
                    .fontSize = 16,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT_LIGHT
                }));
            }
        }

        // Divider
        CLAY(CLAY_ID("DetailDivider"), {
            .layout = { .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_FIXED(1) } },
            .backgroundColor = COLOR_BORDER
        }) {}

        // Title
        CLAY(CLAY_ID("DetailTitle"), {
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
            CLAY_TEXT(CLAY_STRING(task->title), CLAY_TEXT_CONFIG({
                .fontSize = 18,
                .fontId = FONT_ID_BODY_20,
                .textColor = COLOR_TEXT
            }));
        }

        // Description
        CLAY(CLAY_ID("DetailDesc"), {
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
            CLAY_TEXT(CLAY_STRING(task->description[0] ? task->description : "No description"), CLAY_TEXT_CONFIG({
                .fontSize = 14,
                .fontId = FONT_ID_BODY_16,
                .textColor = COLOR_TEXT
            }));
        }

        // Status
        CLAY(CLAY_ID("DetailStatus"), {
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
            CLAY(CLAY_ID("DetailStatusBadge"), {
                .layout = {
                    .sizing = { CLAY_SIZING_FIT(0), CLAY_SIZING_FIT(0) },
                    .padding = { 10, 10, 6, 6 }
                },
                .backgroundColor = GetStatusColor(task->status),
                .cornerRadius = CLAY_CORNER_RADIUS(4)
            }) {
                CLAY_TEXT(CLAY_STRING(GetStatusText(task->status)), CLAY_TEXT_CONFIG({
                    .fontSize = 14,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT_WHITE
                }));
            }
        }

        // Priority
        CLAY(CLAY_ID("DetailPriority"), {
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
            CLAY(CLAY_ID("DetailPriorityRow"), {
                .layout = {
                    .sizing = { CLAY_SIZING_FIT(0), CLAY_SIZING_FIT(0) },
                    .childGap = 8,
                    .childAlignment = { .y = CLAY_ALIGN_Y_CENTER }
                }
            }) {
                CLAY(CLAY_ID("DetailPriorityDot"), {
                    .layout = { .sizing = { CLAY_SIZING_FIXED(10), CLAY_SIZING_FIXED(10) } },
                    .backgroundColor = GetPriorityColor(task->priority),
                    .cornerRadius = CLAY_CORNER_RADIUS(5)
                }) {}
                CLAY_TEXT(CLAY_STRING(GetPriorityText(task->priority)), CLAY_TEXT_CONFIG({
                    .fontSize = 14,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT
                }));
            }
        }

        // Due date
        if (task->due_date[0] != '\0') {
            CLAY(CLAY_ID("DetailDue"), {
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
                CLAY_TEXT(CLAY_STRING(task->due_date), CLAY_TEXT_CONFIG({
                    .fontSize = 14,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT
                }));
            }
        }

        // Assigned to
        if (task->assigned_to[0] != '\0') {
            CLAY(CLAY_ID("DetailAssigned"), {
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
                CLAY_TEXT(CLAY_STRING(task->assigned_to), CLAY_TEXT_CONFIG({
                    .fontSize = 14,
                    .fontId = FONT_ID_BODY_16,
                    .textColor = COLOR_TEXT
                }));
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

            // Username input placeholder (actual input is HTML overlay)
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
    CLAY(CLAY_ID("MainContainer"), {
        .layout = {
            .sizing = { CLAY_SIZING_GROW(0), CLAY_SIZING_GROW(0) }
        }
    }) {
        Sidebar();
        TaskList();
        DetailPanel();
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

// WASM exports
CLAY_WASM_EXPORT("SetScratchMemory") void SetScratchMemory(void* memory) {
    frame_arena.memory = memory;
}

CLAY_WASM_EXPORT("UpdateDrawFrame") Clay_RenderCommandArray UpdateDrawFrame(
    uint32_t scratch_address,
    float width, float height,
    float mouse_wheel_x, float mouse_wheel_y,
    float mouse_x, float mouse_y,
    bool touch_down, bool mouse_down,
    float delta_time
) {
    frame_arena.offset = 0;
    window_width = width;
    window_height = height;

    Clay_SetLayoutDimensions((Clay_Dimensions){width, height});
    Clay_SetPointerState((Clay_Vector2){mouse_x, mouse_y}, mouse_down || touch_down);
    Clay_UpdateScrollContainers(touch_down, (Clay_Vector2){mouse_wheel_x, mouse_wheel_y}, delta_time);

    return CreateLayout();
}

// JS interop functions
CLAY_WASM_EXPORT("GetAppState") AppState* GetAppState(void) {
    return &app_state;
}

CLAY_WASM_EXPORT("SetLoggedIn") void SetLoggedIn(bool logged_in) {
    app_state.logged_in = logged_in;
}

CLAY_WASM_EXPORT("SetCurrentUser") void SetCurrentUser(const char* username) {
    // Copy username (JS will write directly to memory)
}

CLAY_WASM_EXPORT("AddTask") void AddTask(
    uint32_t id,
    uint32_t status,
    uint32_t priority
) {
    if (app_state.task_count < 100) {
        Task* task = &app_state.tasks[app_state.task_count];
        task->id = id;
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
    app_state.show_create_modal = false; // Auto-reset
    return result;
}

CLAY_WASM_EXPORT("InitApp") void InitApp(void) {
    app_state.logged_in = false;
    app_state.task_count = 0;
    app_state.selected_task_index = -1;
    app_state.filter_status = FILTER_ALL;
    app_state.show_create_modal = false;
    app_state.show_detail_panel = false;
}

// Dummy main for WASM
int main(void) {
    return 0;
}
