using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Text;
using System.Text.RegularExpressions;
using System.Xml;
using System.Xml.Linq;
using WixSharp;

class Script
{
    enum Target
    {
        i686,
        x86_64,
    }

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

    static Target ReadTarget(string path)
    {
        String target = System.IO.File.ReadAllText(path);
        switch (target)
        {
            case "x86_64-pc-windows-gnu":
                return Target.x86_64;
            case "i686-pc-windows-gnu":
                return Target.i686;
            default:
                throw new Exception("Unknown target: " + target);
        }
    }

    static void CreateNuspec(string template, string output, string version, Target target)
    {
        string content = System.IO.File.ReadAllText(template, Encoding.UTF8);
        content = content.Replace("$version$", version);
        content = content.Replace("$target$", target.ToString());
        System.IO.File.WriteAllText(output, content, Encoding.UTF8);
    }

    static public void Main(string[] args)
    {
        Console.WriteLine("WixSharp version: " + FileVersionInfo.GetVersionInfo(typeof(WixSharp.Project).Assembly.Location).FileVersion);

        Target target = ReadTarget(@"target\release\target.txt");
        String version = ReadVersion(@"Cargo.toml");
        Feature featureBuilder = new Feature("Octobuild Builder", true, false);
        featureBuilder.AttributesDefinition = @"AllowAdvertise=no";
        String programFile = (target == Target.x86_64) ? "%ProgramFiles64Folder%" : "%ProgramFilesFolder%";

        List<WixEntity> files = new List<WixEntity>();
        files.Add(new File(featureBuilder, @"target\release\xgconsole.exe"));
        files.Add(new File(featureBuilder, @"LICENSE"));
        foreach (string file in System.IO.Directory.GetFiles(@"target\release", "*.dll"))
        {
            files.Add(new File(featureBuilder, file));
        }
        Project project = new Project("Octobuild",
            new Property("ApplicationFolderName", "Octobuild"),
            new Property("WixAppFolder", "WixPerMachineFolder"),
            new Dir(new Id("APPLICATIONFOLDER"), programFile + @"\Octobuild", files.ToArray()),
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
        );
        project.LicenceFile = @"LICENSE.rtf";
        project.LicenceFile = @"LICENSE.rtf";
        project.GUID = new Guid("b4505233-6377-406b-955b-2547d86a99a7");
        project.UI = WUI.WixUI_Advanced;
        project.Version = new Version(version);
        project.Manufacturer = "Artem V. Navrotskiy";
        project.OutFileName = @"target\octobuild-" + version + "-" + target;
        project.Package.AttributesDefinition = @"InstallPrivileges=elevated;InstallScope=perMachine";

        if (target == Target.x86_64)
        {
            project.Package.AttributesDefinition += @";Platform=x64";
            Compiler.WixSourceGenerated += new XDocumentGeneratedDlgt(Compiler_WixSourceGenerated);
        }

        Compiler.BuildMsi(project);
        Compiler.BuildWxs(project);
        CreateNuspec(@"wixcs\octobuild.nuspec", @"target\octobuild.nuspec", version, target);
    }

    static void Compiler_WixSourceGenerated(XDocument document)
    {
        foreach (XElement comp in document.Root.AllElements("Component"))
        {
            comp.Add(new XAttribute("Win64", "yes"));
        }
    }
}
