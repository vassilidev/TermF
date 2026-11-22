; Uninstalling TermF, not updating it, takes its folder out of the user's PATH, where Settings may have put it for
; the termf CLI (src/cli_path.rs: same comparison, case and trailing backslash aside, empty entries dropped).
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    System::Call 'kernel32::SetEnvironmentVariable(t "TERMF_DIR", t "$INSTDIR")'
    nsExec::Exec `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$$k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $$true); if ($$k) { $$v = $$k.GetValue('Path', $$null, 'DoNotExpandEnvironmentNames'); if ($$v) { $$d = $$env:TERMF_DIR.Trim().TrimEnd('\', '/'); $$all = @($$v -split ';'); $$keep = @($$all | Where-Object { $$_.Trim().TrimEnd('\', '/') -ne $$d }); if ($$keep.Count -ne $$all.Count) { $$k.SetValue('Path', (@($$keep | Where-Object { $$_ }) -join ';'), 'ExpandString') } } }"`
    Pop $0
    SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
  ${EndIf}
!macroend
