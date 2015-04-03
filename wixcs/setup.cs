using System;
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
		Target target = ReadTarget(@"target\release\target.txt");
		String version = ReadVersion(@"Cargo.toml");
		Feature featureBuilder = new Feature("Octobuild Builder", true, false);
		String programFile = (target == Target.x86_64) ? "%ProgramFiles64Folder%" : "%ProgramFilesFolder%";
		Project project = new Project("Octobuild",
			new Dir(programFile + @"\Octobuild",
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
		project.LicenceFile = @"LICENSE.rtf";
		project.GUID = new Guid("b4505233-6377-406b-955b-2547d86a99a7");
		project.UI = WUI.WixUI_InstallDir;
		project.Version = new Version(version);
		project.Manufacturer = "Artem V. Navrotskiy";
		project.OutFileName = @"target\octobuild-" + version + "-" + target;

		if (target == Target.x86_64)
		{
			project.Package.AttributesDefinition = @"Platform=x64;InstallScope=perMachine";
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
