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


return t.Summary();
