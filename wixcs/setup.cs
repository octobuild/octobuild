using System;
using System.Text.RegularExpressions;
using System.Xml;
using System.Xml.Linq;
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
        Feature featureBuilder = new Feature("Octobuild Builder", true, false);
        Project project = new Project("Octobuild",
            new Dir(@"%ProgramFiles64Folder%\Octobuild",
                new File(featureBuilder, @"target\release\xgconsole.exe"),
                new File(featureBuilder, @"LICENSE")
            ),
            new EnvironmentVariable(featureBuilder, "PATH", "[INSTALLDIR]")
            {
                Permanent = false,
                Part = EnvVarPart.last,
                Action = EnvVarAction.set,
                System = true,
            }
        );
        project.LicenceFile = @"LICENSE.rtf";
        project.GUID = new Guid("b4505233-6377-406b-955b-2547d86a99a7");
        project.UI = WUI.WixUI_InstallDir;
        project.Package.AttributesDefinition = @"Platform=x64;InstallScope=perMachine";
        project.Version = new Version(version);
        project.OutFileName = @"target\octobuild-" + version + "-x86_64";

        Compiler.WixSourceGenerated += new XDocumentGeneratedDlgt(Compiler_WixSourceGenerated);
        Compiler.BuildMsi(project);
        Compiler.BuildWxs(project);
    }

    static void Compiler_WixSourceGenerated(XDocument document)
    {
        foreach (XElement comp in document.Root.AllElements("Component"))
        {
            comp.Add(new XAttribute("Win64", "yes"));
        }
    }
}
