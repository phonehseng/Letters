; extra steps for the windows installer (nsis), run by tauri's installer
; at the points named below. the installer is per user, so SHCTX is
; HKEY_CURRENT_USER and nothing here needs administrator rights.
;
; what it adds to tauri's own file associations (Letters.<ext> classes):
; 1. each type's document icon (from tools/icon/make-document-icons.ps1)
; 2. the "capabilities" windows lists in Settings > Apps > Default apps,
;    so the person can choose Letters there. windows lets only the person
;    pick the default app; an installer must not and does not set it.
; the uninstaller removes exactly what was added.

!macro LETTERS_TYPE EXT CLASS
  WriteRegStr SHCTX "Software\Classes\${CLASS}\DefaultIcon" "" "$INSTDIR\document-icons\${EXT}.ico"
  WriteRegStr SHCTX "Software\Letters\Capabilities\FileAssociations" ".${EXT}" "${CLASS}"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr SHCTX "Software\Letters\Capabilities" "ApplicationName" "Letters"
  WriteRegStr SHCTX "Software\Letters\Capabilities" "ApplicationDescription" "A PDF editor for students: read, mark up, fill in, convert and write documents."
  WriteRegStr SHCTX "Software\Letters\Capabilities" "ApplicationIcon" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  !insertmacro LETTERS_TYPE "pdf" "Letters.pdf"
  !insertmacro LETTERS_TYPE "docx" "Letters.docx"
  !insertmacro LETTERS_TYPE "odt" "Letters.odt"
  !insertmacro LETTERS_TYPE "ods" "Letters.ods"
  !insertmacro LETTERS_TYPE "odp" "Letters.odp"
  !insertmacro LETTERS_TYPE "rtf" "Letters.rtf"
  !insertmacro LETTERS_TYPE "png" "Letters.png"
  !insertmacro LETTERS_TYPE "jpg" "Letters.jpg"
  WriteRegStr SHCTX "Software\Letters\Capabilities\FileAssociations" ".jpeg" "Letters.jpg"
  !insertmacro LETTERS_TYPE "tif" "Letters.tif"
  WriteRegStr SHCTX "Software\Letters\Capabilities\FileAssociations" ".tiff" "Letters.tif"
  !insertmacro LETTERS_TYPE "bmp" "Letters.bmp"
  !insertmacro LETTERS_TYPE "gif" "Letters.gif"
  !insertmacro LETTERS_TYPE "webp" "Letters.webp"
  WriteRegStr SHCTX "Software\RegisteredApplications" "Letters" "Software\Letters\Capabilities"
  ; tell explorer the associations changed, so icons refresh
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue SHCTX "Software\RegisteredApplications" "Letters"
  DeleteRegKey SHCTX "Software\Letters\Capabilities"
  DeleteRegKey /ifempty SHCTX "Software\Letters"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend
