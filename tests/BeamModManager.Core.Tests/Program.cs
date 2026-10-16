using BeamModManager.Core.Tests;

var t = new TestRunner();

TitleTests.Register(t);
PricingTests.Register(t);
VersionTests.Register(t);
HostLinkTests.Register(t);
MatcherTests.Register(t);
ArchiveTests.Register(t);
ModlandTests.Register(t);
ModsGamingTests.Register(t);
RealPageTests.Register(t);
SourceStatusTests.Register(t);
UpdatePlannerTests.Register(t);
NetTests.Register(t);
InstallTests.Register(t);
SearchTests.Register(t);
AdapterTests.Register(t);
        PackTests.Register(t);
        UpdateRunnerTests.Register(t);
        UpdateCoordinatorTests.Register(t);
        SettingsTests.Register(t);
        BugHuntTests.Register(t);
        if (Environment.GetEnvironmentVariable("BMM_SKIP_FUZZ") != "1") FuzzTests.Register(t); // долгий набор: BMM_SKIP_FUZZ=1 пропускает
Revision2Tests.Register(t);
RepairTests.Register(t);
TimeTests.Register(t);
ScenarioTests.Register(t);
SyncTests.Register(t);
DownloadPolicyTests.Register(t);
LibraryTests.Register(t);
CoreFixesTests.Register(t);
GameDbTests.Register(t);
GameRepoFolderTests.Register(t);
GameDateAddedTests.Register(t);
GameBridgeTests.Register(t);
GameDbRobustnessTests.Register(t);
AppUpdateTests.Register(t);
JsonStoreTests.Register(t);
JsonStoreFailureTests.Register(t);
PreviewTests.Register(t);
PreviewLoaderTests.Register(t);
GarageTests.Register(t);
GarageCardTests.Register(t);
SwitchingBackendTests.Register(t);
SettingsScreenTests.Register(t);
UpdatesScreenTests.Register(t);
SiteChangeTests.Register(t);
BackgroundThemeTests.Register(t);


GlassTextureTests.Register(t);
return t.Summary();
