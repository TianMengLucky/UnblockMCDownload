using System;
using System.Diagnostics;
using System.IO;
using PCL.Core.App.Region;

class Program
{
    static void Main(string[] args)
    {
        var result = "";
        foreach (var classification in Enum.GetValues<RegionClassification>())
            result += $"{classification}:{new RegionAssessment(classification, "fixture").IsRestricted}\n";
        if (args.Length == 3 && args[0] == "simulate-update")
        {
            PCL.UpdateManager.Source = args[1];
            PCL.UpdateManager.Target = args[2];
            PCL.UpdateManager.UpdateRestart(true, false);
        }
        else if (args.Length == 5 && args[0] == "update")
            File.WriteAllText(Path.Combine(Path.GetDirectoryName(args[2])!, "update-result.txt"), result);
        else Console.Write(result);
    }
}

namespace PCL
{
    static class ModBase
    {
        // Match the upstream logger's four arguments, including its optional defaults.
        internal static void Log(string value, int level = 0, string? title = null, string? userSummary = null)
        { Console.Error.WriteLine(value); }
    }

    static class UpdateManager
    {
        internal static string Source = "";
        internal static string Target = "";

        public static void UpdateRestart(bool triggerRestartAndByEnd, bool triggerRestart = true)
        {
            var fileName = Source;
            if (!File.Exists(fileName)) { ModBase.Log("Update file not found"); return; }
            var text = $"update {Process.GetCurrentProcess().Id} \"{Target}\" \"{fileName}\" {(triggerRestart ? "true" : "false")}";
            ModBase.Log("Starting updater: " + text);
            Process.Start(new ProcessStartInfo(fileName)
            {
                WindowStyle = ProcessWindowStyle.Hidden,
                CreateNoWindow = true,
                Arguments = text
            });
        }
    }
}
