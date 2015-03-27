using System;
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

    static public void Main(string[] args)
    {
        String version = ReadVersion(@"Cargo.toml");
        EnvironmentVariable path = new EnvironmentVariable("PATH", "[INSTALLLOCATION]");
        path.Permanent = false;
        path.Part = EnvVarPart.last;
        path.Action = EnvVarAction.set;
        path.System = true;
        Project project = new Project("Octobuild",
            new Dir(@"%ProgramFiles64Folder%\octobuild",
                new File(@"target\release\xgconsole.exe"),
                new File(@"LICENSE-MIT")
            ),
            path
        );
        project.LicenceFile = @"LICENSE.rtf";
        project.GUID = new Guid("b4505233-6377-406b-955b-2547d86a99a7");
        project.UI = WUI.WixUI_InstallDir;
        project.Package.AttributesDefinition = "Platform=x64";
        project.Version = new Version(version);
        project.OutFileName = @"target\octobuild-" + version + "-x86_64";

        foreach (var file in project.AllFiles)
        {
            file.Attributes.Add("Component:Win64", "yes");
        }

        Compiler.BuildMsi(project);
    }
}
