//css_dir %WIXSHARP_DIR%;
//css_ref Wix_bin\SDK\Microsoft.Deployment.WindowsInstaller.dll";
//css_ref System.Core.dll;
using Microsoft.Deployment.WindowsInstaller;
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;
using WixSharp;

class Script
{
    static public string ReadVersion(string path)
    {
        System.IO.StreamReader file = new System.IO.StreamReader(path);
        Regex regex = new Regex("^\\s*version\\s*=\\s*\"(\\S+)\"");
        string line;
        while ((line = file.ReadLine()) != null)
        {
            Match match = regex.Match(line);
            if (match.Success)
            {
                return match.Groups[1].Value;
            }
        }
        return null;
    }

    static Platform ReadPlatform(string path)
    {
        String target = System.IO.File.ReadAllText(path);
        switch (target)
        {
            case "x86_64-pc-windows-gnu":
                return Platform.x64;
            case "i686-pc-windows-gnu":
                return Platform.x86;
            default:
                throw new Exception("Unknown target: " + target);
        }
    }

    static string PlatformName(Platform platform)
    {
        switch (platform)
        {
            case Platform.x64:
                return "x86_64";
            case Platform.x86:
                return "i686";
            default:
                throw new Exception("Unknown platform: " + platform);
        }
    }

    static void CreateNuspec(string template, string output, string version)
    {
        string content = System.IO.File.ReadAllText(template, Encoding.UTF8);
        content = content.Replace("$version$", version);
        System.IO.Directory.CreateDirectory(System.IO.Path.GetDirectoryName(output));
        System.IO.File.WriteAllText(output, content, Encoding.UTF8);
    }

    static public void Main(string[] args)
    {
        Console.WriteLine("WixSharp version: " + FileVersionInfo.GetVersionInfo(typeof(WixSharp.Project).Assembly.Location).FileVersion);

        Platform platform = ReadPlatform(@"target\release\target.txt");
        String version = ReadVersion(@"Cargo.toml");
        Feature featureBuilder = new Feature("Octobuild Builder", true, false);
        featureBuilder.AttributesDefinition = @"AllowAdvertise=no";

        List<WixEntity> files = new List<WixEntity>();
        files.Add(new File(featureBuilder, @"target\release\xgConsole.exe"));
        files.Add(new File(featureBuilder, @"LICENSE"));
        foreach (string file in System.IO.Directory.GetFiles(@"target\release", "*.dll"))
        {
            files.Add(new File(featureBuilder, file));
        }

        List<WixEntity> projectEntries = new List<WixEntity>();
        projectEntries.AddRange(new WixEntity[] {
            new Property("ApplicationFolderName", "Octobuild"),
            new Property("WixAppFolder", "WixPerMachineFolder"),
            new Dir(new Id("APPLICATIONFOLDER"), @"%ProgramFiles%\Octobuild", files.ToArray()),
            new EnvironmentVariable(featureBuilder, "PATH", "[APPLICATIONFOLDER]")
            {
                Permanent = false,
                Part = EnvVarPart.last,
                Action = EnvVarAction.set,
                System = true,
                Condition = new Condition("ALLUSERS")
            },
            new EnvironmentVariable(featureBuilder, "PATH", "[APPLICATIONFOLDER]")
            {
                Permanent = false,
                Part = EnvVarPart.last,
                Action = EnvVarAction.set,
                System = false,
                Condition = new Condition("NOT ALLUSERS")
            }
        });

        // Workarong for bug with invalid default installation path "C:\Program Files (x86)" for x86_64 platform.
        if (platform == Platform.x64)
        {
            foreach (Sequence sequence in new Sequence[] { Sequence.InstallUISequence, Sequence.InstallExecuteSequence })
            {
                projectEntries.Add(
                    new SetPropertyAction("WixPerMachineFolder", "[ProgramFiles64Folder][ApplicationFolderName]")
                    {
                        Execute = Execute.immediate,
                        When = When.After,
                        Sequence = sequence,
                        Step = new Step("WixSetDefaultPerMachineFolder")
                    }
                );
            }
        }
	projectEntries.Add(new ManagedAction(@"BroadcastSettingChange", Return.ignore, When.After, Step.InstallFinalize, Condition.Always));

        Project project = new Project("Octobuild", projectEntries.ToArray());
        project.ControlPanelInfo.Manufacturer = "Artem V. Navrotskiy";
        project.ControlPanelInfo.UrlInfoAbout = "https://github.com/bozaro/octobuild";
        project.LicenceFile = @"LICENSE.rtf";
        project.LicenceFile = @"LICENSE.rtf";
        project.GUID = new Guid("b4505233-6377-406b-955b-2547d86a99a7");
        project.UI = WUI.WixUI_Advanced;
        project.Version = new Version(version);
        project.OutFileName = @"target\octobuild-" + version + "-" + PlatformName(platform);
        project.Platform = Platform.x64;
        project.Package.AttributesDefinition = @"InstallPrivileges=elevated;InstallScope=perMachine";
        project.MajorUpgradeStrategy = MajorUpgradeStrategy.Default;

        Compiler.BuildMsi(project);
        //Compiler.BuildWxs(project);
        CreateNuspec(@"choco\octobuild.nuspec", @"target\choco\octobuild.nuspec", version);
        CreateNuspec(@"choco\tools\chocolateyInstall.ps1", @"target\choco\tools\chocolateyInstall.ps1", version);
    }
}

public class CustomActions
{
	[DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Auto)]
	static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam, SendMessageTimeoutFlags fuFlags, uint uTimeout, out UIntPtr lpdwResult);

	public enum SendMessageTimeoutFlags : uint
{
   SMTO_NORMAL = 0x0, SMTO_BLOCK = 0x1, SMTO_ABORTIFHUNG = 0x2, SMTO_NOTIMEOUTIFNOTHUNG = 0x8
}

    [CustomAction]
    public static ActionResult BroadcastSettingChange(Session session)
    {
		IntPtr HWND_BROADCAST = (IntPtr)0xffff;
		const UInt32 WM_SETTINGCHANGE = 0x001A;
		UIntPtr result;
		SendMessageTimeout(HWND_BROADCAST, WM_SETTINGCHANGE, (UIntPtr)0, "Environment", SendMessageTimeoutFlags.SMTO_ABORTIFHUNG, 5000, out result);
        return ActionResult.Success;
    }
}
