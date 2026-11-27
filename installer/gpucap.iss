; GPUCap Windows x64 安装包脚本（Inno Setup 6）
#ifndef AppVersion
  #define AppVersion "0.2.0"
#endif

[Setup]
AppId={{7A9C4D8E-2B1F-4E6D-9A0C-3F5E8B1D2C4A}
AppName=GPUCap
AppVersion={#AppVersion}
AppVerName=GPUCap {#AppVersion}
AppPublisher=GPUCap
DefaultDirName={autopf}\GPUCap
DefaultGroupName=GPUCap
AllowNoIcons=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=..\dist-win
OutputBaseFilename=gpucap-setup-x64
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
SetupIconFile=..\assets\gpucap.ico
UninstallDisplayIcon={app}\gpucap.exe
AppMutex=Global\GPUCapUiSingletonMutex
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "autostart"; Description: "Launch GPUCap minimized to tray at Windows startup"; GroupDescription: "Startup Options:"; Flags: unchecked

[Dirs]
Name: "{app}\data"; Permissions: users-modify

[Files]
Source: "..\target\release\gpucap.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.zh-CN.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\GPUCap"; Filename: "{app}\gpucap.exe"
Name: "{group}\{cm:UninstallProgram,GPUCap}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\GPUCap"; Filename: "{app}\gpucap.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "GPUCap"; ValueData: """{app}\gpucap.exe"" --tray"; Flags: uninsdeletevalue; Tasks: autostart

[Run]
Filename: "{app}\gpucap.exe"; Description: "{cm:LaunchProgram,GPUCap}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "taskkill"; Parameters: "/F /IM gpucap.exe"; Flags: runhidden; RunOnceId: "KillGPUCap"
