/**
 * IRONCLAD keybind module
 * Keeps keyboard mapping separate from rendering/engine internals.
 */

(function () {
    const ACTIONS = {
        ArrowLeft:  { id: 'timeline.prev' },
        ArrowRight: { id: 'timeline.next' },
        'Alt+m':    { id: 'view.toggleMonth' },
        'Alt+c':    { id: 'view.toggleCollapseOrDay' },
    };

    function actionForEvent(e) {
        if (e.key === 'ArrowLeft') return ACTIONS.ArrowLeft;
        if (e.key === 'ArrowRight') return ACTIONS.ArrowRight;
        if (!e.altKey) return null;

        const k = 'Alt+' + e.key.toLowerCase();
        return ACTIONS[k] || null;
    }

    function handleKeydown(engine, e) {
        const action = actionForEvent(e);
        if (!action) return false;

        switch (action.id) {
            case 'timeline.prev':
                e.preventDefault();
                engine._nudgeTimeline(-1);
                return true;

            case 'timeline.next':
                e.preventDefault();
                engine._nudgeTimeline(1);
                return true;

            case 'view.toggleMonth':
                e.preventDefault();
                engine.viewMode = engine.viewMode === 'month' ? 'week' : 'month';
                if (engine.viewMode === 'month') {
                    if (engine.container.classList.contains('collapsed')) {
                        engine.container.classList.remove('collapsed');
                        engine.restoreCollapsedAfterMonth = true;
                    }
                    for (let i = 0; i < engine.pool.length; i++) engine.pool[i].style.display = 'none';
                } else {
                    if (engine.restoreCollapsedAfterMonth) {
                        engine.container.classList.add('collapsed');
                        engine.restoreCollapsedAfterMonth = false;
                    }
                    engine._reflowGridLayout();
                    engine.prevMX = -9999;
                }
                engine.dirty = true;
                return true;

            case 'view.toggleCollapseOrDay':
                e.preventDefault();
                if (engine.viewMode === 'month') {
                    engine.viewMode = 'week';
                    engine.restoreCollapsedAfterMonth = false;
                    engine.container.classList.add('collapsed');
                    engine._reflowGridLayout();
                    engine.prevMX = -9999;
                    engine.dirty = true;
                    return true;
                }

                engine.container.classList.toggle('collapsed');
                engine._reflowGridLayout();
                engine.prevMX = -9999;
                engine.dirty = true;
                return true;

            default:
                return false;
        }
    }

    window.IroncladKeybinds = {
        ACTIONS,
        handleKeydown,
    };
})();
