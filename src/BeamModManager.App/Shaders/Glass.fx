// Преломление «жидкого стекла» (DESIGN.md 3.2). Тот же расчёт, что в макете:
// карта смещения из design/mockup/tools/gen.py (make) и feDisplacementMap из build_ui.py (lens).
// Смещение в пикселях = strength * 0.5 (halfStrength) * нормаль внутрь * t^2, t = clamp(1 - d / lens, 0, 1) (1 / lens считает RefractionEffect),
// d — расстояние от края скруглённого прямоугольника внутрь. Каналы R, G, B сдвигаются с множителями
// 1.0 / 1.12 / 1.26: на краю тонкая радужная кромка. Затем насыщенность и яркость, как saturate() и brightness() в CSS,
// и «подъём» для светлого оформления: тёмное под стеклом высветляется сильнее светлого (2026-10-17). Так светлое стекло
// остаётся прозрачным над светлым фоном, а тёмный текст читается и над тёмным фото.
// Все длины в одних единицах (точки WPF): от единиц зависит только их отношение.
// Сборка: fxc /T ps_2_0 /E main (PS 2.0 WPF умеет рисовать и без видеокарты, программно) (цель CompileGlassShader в BeamModManager.App.csproj).

sampler2D input : register(s0);

// всё, что одинаково для всех точек панели, считает RefractionEffect (у PS 2.0 всего 64 арифметические команды)
float2 size : register(c0);       // размер панели (не меньше 1)
float4 corner : register(c1);     // x — радиус скругления (не больше половины меньшей стороны); yz — половина размера минус радиус
float invLens : register(c2);     // 1 / ширина линзы у края
float halfStrength : register(c3); // половина силы: сдвиг у самого края (scale из макета, уже с множителем)
float saturation : register(c4);  // насыщенность, 1 — без изменений
float brightness : register(c5);  // яркость, 1 — без изменений
float lift : register(c6);        // подъём тёмного к белому (светлое оформление), 0 — без изменений
float shade : register(c7);       // тень в толще у края (светлое оформление), уже умноженная на яркость: видно, что край выпуклый
float4 edge : register(c8);       // xy — нижняя, zw — верхняя граница выборок в долях: на точку от края картинки. На самом краю
                                  // видеокарта подмешивает прозрачное, и у сильной линзы капли были серая «шапка» и оранжевая
                                  // полоса (2026-10-18)
float4 geo : register(c9);        // xy — 1 / размер, zw — половина размера

static const float3 Luma = float3(0.2126, 0.7152, 0.0722);

float4 main(float2 uv : TEXCOORD) : COLOR
{
    // расстояние до скруглённого прямоугольника: внутри d >= 0
    float2 p = uv * size - geo.zw;
    float2 sg = sign(p);
    float2 q = abs(p) - corner.yz;
    float d = -(length(max(q, float2(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - corner.x);

    // нормаль внутрь: в углу — от центра скругления, на прямом участке — по оси ближнего края
    float2 nCorner = -sg * normalize(max(q, float2(0.0001, 0.0001)));
    float2 nSide = (q.x > q.y) ? float2(-sg.x, 0.0) : float2(0.0, -sg.y);
    float2 n = (min(q.x, q.y) > 0.0) ? nCorner : nSide;

    float t = saturate(1.0 - d * invLens);
    float tt = t * t;
    float2 off = n * (halfStrength * tt) * geo.xy;   // в долях текстуры

    float4 c = tex2D(input, uv);
    float cr = tex2D(input, clamp(uv + off, edge.xy, edge.zw)).r;
    float cg = tex2D(input, clamp(uv + off * 1.12, edge.xy, edge.zw)).g;
    float cb = tex2D(input, clamp(uv + off * 1.26, edge.xy, edge.zw)).b;

    // цвета с умноженной прозрачностью: обе операции линейные, остаётся только не выйти за альфу
    float3 col = float3(cr, cg, cb);
    float l = dot(col, Luma);
    col = lerp(float3(l, l, l), col, saturation);
    float a = c.a;
    // l не больше a (цвета с умноженной прозрачностью), поэтому 1 - l без saturate; яркость и тень у края — одним множителем
    col = lerp(col, float3(a, a, a), lift - lift * l);
    col *= brightness - shade * tt;
    // не выйти за альфу обязательно: у капли и у края копии сцены бывают полупрозрачные точки, и цвет больше альфы давал
    // светлые полосы (0.1.122, «полосы в каплях»)
    col = min(max(col, float3(0.0, 0.0, 0.0)), float3(a, a, a));

    // снаружи прозрачно, на самом краю полпикселя сглаживания
    float cover = saturate(d + 0.5);
    return float4(col, a) * cover;
}
