/* Companion process for Fireworks CS6 under Wine (built with i686-w64-mingw32-gcc by cs6_lib.sh).
 *
 *   cs6helper.exe [nofonts]
 *
 * Mouse drags: on a button press CS6 hands the stroke to a worker thread, which calls SetCapture on
 * its own overlay window (class {ED22A48D-...}, an owned top-level tool window). Windows attaches
 * the input queues of threads whose windows are parent/child or owner/owned, so that capture takes
 * the mouse. Wine 9 only attaches for child windows, so the capture stays in the worker's queue,
 * every WM_MOUSEMOVE/WM_LBUTTONUP goes to the canvas window on the UI thread, and the canvas ignores
 * them: drags do nothing and strokes are left open. Calling AttachThreadInput for each overlay and
 * its owner, as Windows would have done, fixes it.
 *
 * nofonts: answer the "Missing Fonts" dialog with "No Change" (for unattended scripts).
 *
 * Polls once a second; exits when the Fireworks process it attached to exits, or after two minutes
 * if no Fireworks overlay window ever appears.
 */
#include <windows.h>
#include <stdio.h>
#include <string.h>

static const char *OVERLAY_CLASS = "{ED22A48D-C38C-46da-B437-268E58338B68}";
static DWORD attached[256][2];
static int nattached;
static HANDLE fw_process;
static int nofonts;

static BOOL CALLBACK find_no_change(HWND w, LPARAM lp)
{
    char text[64];
    if (GetWindowTextA(w, text, sizeof text) && !strcmp(text, "&No Change")) {
        *(HWND *)lp = w;
        return FALSE;
    }
    return TRUE;
}

static BOOL CALLBACK each_window(HWND w, LPARAM lp)
{
    char cls[64], title[64];
    (void)lp;
    if (nofonts && GetWindowTextA(w, title, sizeof title) && !strcmp(title, "Missing Fonts")
        && IsWindowVisible(w)) {
        HWND button = NULL;
        EnumChildWindows(w, find_no_change, (LPARAM)&button);
        if (button) {
            PostMessageA(w, WM_COMMAND, MAKEWPARAM(GetDlgCtrlID(button), BN_CLICKED), (LPARAM)button);
            printf("Missing Fonts: No Change\n");
            fflush(stdout);
        }
        return TRUE;
    }
    if (!GetClassNameA(w, cls, sizeof cls) || strcmp(cls, OVERLAY_CLASS)) return TRUE;
    HWND owner = GetWindow(w, GW_OWNER);
    if (!owner) return TRUE;
    DWORD pid;
    DWORD t1 = GetWindowThreadProcessId(w, &pid), t2 = GetWindowThreadProcessId(owner, NULL);
    if (!fw_process) fw_process = OpenProcess(SYNCHRONIZE, FALSE, pid);
    if (t1 == t2) return TRUE;
    for (int i = 0; i < nattached; i++)
        if (attached[i][0] == t1 && attached[i][1] == t2) return TRUE;
    BOOL ok = AttachThreadInput(t1, t2, TRUE);
    printf("overlay %p (thread %04lx) -> owner %p (thread %04lx): %s\n", (void *)w, t1, (void *)owner,
           t2, ok ? "input attached" : "AttachThreadInput FAILED");
    fflush(stdout);
    if (ok && nattached < 256) {
        attached[nattached][0] = t1;
        attached[nattached][1] = t2;
        nattached++;
    }
    return TRUE;
}

int main(int argc, char **argv)
{
    for (int i = 1; i < argc; i++)
        if (!strcmp(argv[i], "nofonts")) nofonts = 1;
    for (int ticks = 0;; ticks++) {
        EnumWindows(each_window, 0);
        if (fw_process) {
            if (WaitForSingleObject(fw_process, 1000) == WAIT_OBJECT_0) return 0;
        } else {
            if (ticks > 120) return 1;
            Sleep(1000);
        }
    }
}
