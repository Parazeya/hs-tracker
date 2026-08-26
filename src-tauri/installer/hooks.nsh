; HS Tracker reads the game's traffic through Npcap, so the installer checks for
; it and says where to get it.
;
; It does not fetch it. It used to: it ran PowerShell with -ExecutionPolicy
; Bypass to download npcap.exe into %TEMP% and then executed it. Every step of
; that is defensible on its own and the four of them together are the shape of a
; dropper — an NSIS installer spawning PowerShell with the execution policy off,
; writing a program into the temp folder and running it. Behavioural engines
; match that chain without ever looking at what is inside, and Defender has
; attack-surface rules aimed squarely at it. A tracker that reads network
; traffic starts with enough against it; it does not need to arrive looking like
; this as well.
;
; Npcap cannot be bundled either — its free edition may not be redistributed
; inside another installer, and only the paid OEM edition has a silent mode. So
; the honest version of this is what a user would do anyway: say what is
; missing, and open the page it comes from.

; Do not set `publisher` in tauri.conf.json.
;
; It reads like a label and is not one. Tauri builds `MANUFACTURER` from it and
; `MANUKEY` from that — `Software\${MANUFACTURER}` — and the install directory
; of every version ever installed is the default value under
; `Software\hstracker\HS Tracker`, put there because `MANUFACTURER` falls back
; to the second element of the bundle identifier. Setting it to anything else
; points the lookup at a key that does not exist, so `_?=` is handed an empty
; directory when the installer runs the old uninstaller:
;
;   ReadRegStr $4 SHCTX "${MANUPRODUCTKEY}" ""     ; empty
;   StrCpy $R1 "$R1 _?=$4"                         ; uninstall.exe _?=
;
; which fails with "Error launching installer", and the installer then says
; "Unable to uninstall!" and will not go on. Every existing install upgrades
; through that path. `copyright`, `license` and `longDescription` are safe;
; this one is not.

; the welcome page is generic by default, and it is the first thing anyone
; sees — say what the thing is and what it needs. Defined here because Tauri
; includes this file before it lays out the pages.
!define MUI_WELCOMEPAGE_TITLE "HS Tracker"
!define MUI_WELCOMEPAGE_TEXT "A session overlay for Hero Siege: gold, experience, kills and every drop worth hearing about, shown on top of the game.$\r$\n$\r$\nIt reads the game's network traffic through Npcap and never touches the game itself. Npcap is a separate free download; if it is missing, this installer will point you at it.$\r$\n$\r$\nNothing is sent anywhere — every number stays on this machine.$\r$\n$\r$\nClick Next to continue."

!define NPCAP_PAGE "https://npcap.com/#download"

!macro NSIS_HOOK_POSTINSTALL
  ; the driver installs its DLL beside the system ones, or into its own folder
  ; when WinPcap compatibility is switched off
  ${IfNot} ${FileExists} "$SYSDIR\Npcap\wpcap.dll"
  ${AndIfNot} ${FileExists} "$SYSDIR\wpcap.dll"
    ; The warning about the Administrators box is not padding: a player ticked
    ; it, the driver refused every adapter, and the app told them Npcap was not
    ; installed — so they installed it again.
    MessageBox MB_YESNO|MB_ICONINFORMATION \
      "HS Tracker needs Npcap to read Hero Siege's network traffic, and it is not installed.$\r$\n$\r$\nOpen the Npcap download page now?$\r$\n$\r$\nInstall it with the options it comes with. In particular leave $\"Restrict Npcap driver$\'s access to Administrators only$\" unticked — with it on, HS Tracker is refused the adapter and counts nothing." \
      /SD IDNO IDNO npcap_skipped

    DetailPrint "Opening the Npcap download page..."
    ExecShell "open" "${NPCAP_PAGE}"
  ${EndIf}
  npcap_skipped:
!macroend

; "Delete application data" has to delete this application's data.
;
; The box is Tauri's and so is what it clears: `$APPDATA\${BUNDLEID}` and
; `$LOCALAPPDATA\${BUNDLEID}`, the places a Tauri app usually keeps things. This
; one keeps them beside the executable instead — `data_dir()` on Windows is the
; exe's own folder, which is what makes a portable copy work — so ticking the box
; reported success and left every one of them behind: the settings, the whole run
; history, the sounds the player imported, the log, and `debug-capture.jsonl`,
; which holds an account id and a character name. Reinstalling later brought the
; lot back, to someone who had asked for it to be gone.
;
; Guarded the same way Tauri guards its own block, so an upgrade keeps
; everything: only when the box is ticked and this is not an update.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    Delete "$INSTDIR\settings.json"
    Delete "$INSTDIR\runs.json"
    Delete "$INSTDIR\positions.json"
    Delete "$INSTDIR\carried.json"
    Delete "$INSTDIR\shopping.json"
    ; A file that stops parsing is kept rather than thrown away, under its
    ; own name with `.bad` on the end — see `read_json_or_default`. It holds
    ; whatever the file held, so it is the same data and goes the same way.
    Delete "$INSTDIR\*.json.bad"
    ; written by 0.9.x and not since; still beside the exe on anything upgraded
    Delete "$INSTDIR\sessions.json"
    Delete "$INSTDIR\hs-tracker.log"
    Delete "$INSTDIR\hs-tracker.log.1"
    Delete "$INSTDIR\debug-capture.jsonl"
    Delete "$INSTDIR\debug-capture.old.jsonl"
    ; marks the app leaves to remember a start that went wrong
    Delete "$INSTDIR\soft-render"
    Delete "$INSTDIR\no-paint"
    Delete "$INSTDIR\.write-probe"
    RMDir /r "$INSTDIR\sounds"
    ; empty-only: anything the user put here themselves stays, and so does the
    ; uninstaller that is running out of this folder right now
    RMDir "$INSTDIR"
  ${EndIf}
!macroend
