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
