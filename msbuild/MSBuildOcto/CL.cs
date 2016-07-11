namespace ru.bozaro.octobuild.CPPTasks
{
	using CLTask = Microsoft.Build.CPPTasks.CL;

	public class CL : CLTask
	{
		protected override string GenerateFullPathToTool()
		{
			return "octo_cl.exe";
		}
	}
}