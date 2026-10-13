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
LibraryTests.Register(t);


return t.Summary();
