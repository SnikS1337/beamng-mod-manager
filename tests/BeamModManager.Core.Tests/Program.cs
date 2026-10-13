using BeamModManager.Core.Tests;

var t = new TestRunner();

TitleTests.Register(t);
PricingTests.Register(t);
VersionTests.Register(t);
HostLinkTests.Register(t);
MatcherTests.Register(t);
ArchiveTests.Register(t);

return t.Summary();
