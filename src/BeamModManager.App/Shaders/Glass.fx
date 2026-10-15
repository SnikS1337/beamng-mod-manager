// Преломление «жидкого стекла» (DESIGN.md 3.2). Тот же расчёт, что в макете:
// карта смещения из design/mockup/tools/gen.py (make) и feDisplacementMap из build_ui.py (lens).
// Смещение в пикселях = strength * 0.5 * нормаль внутрь * t^2, t = clamp(1 - d / lens, 0, 1),
// d — расстояние от края скруглённого прямоугольника внутрь. Каналы R, G, B сдвигаются с множителями
// 1.0 / 1.12 / 1.26: на краю тонкая радужная кромка. Затем насыщенность и яркость, как saturate() и brightness() в CSS.
// Все длины в одних единицах (точки WPF): от единиц зависит только их отношение.
// Сборка: fxc /T ps_2_0 /E main (PS 2.0 WPF умеет рисовать и без видеокарты, программно) (цель CompileGlassShader в BeamModManager.App.csproj).

sampler2D input : register(s0);

float2 size : register(c0);       // размер панели
float radius : register(c1);      // радиус скругления
float lens : register(c2);        // ширина линзы у края
float strength : register(c3);    // сила (scale из макета, уже с множителем)
float saturation : register(c4);  // насыщенность, 1 — без изменений
float brightness : register(c5);  // яркость, 1 — без изменений

static const float3 Luma = float3(0.2126, 0.7152, 0.0722);

float4 main(float2 uv : TEXCOORD) : COLOR
{
    float2 sz = max(size, float2(1.0, 1.0));
    float2 hs = sz * 0.5;
    float r = clamp(radius, 0.0, min(hs.x, hs.y));
    float bw = max(lens, 0.001);

    // расстояние до скруглённого прямоугольника: внутри d >= 0
    float2 p = uv * sz - hs;
    float2 sg = sign(p);
    float2 q = abs(p) - (hs - r);
    float d = -(length(max(q, float2(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r);

    // нормаль внутрь: в углу — от центра скругления, на прямом участке — по оси ближнего края
    float2 nCorner = -sg * normalize(max(q, float2(0.0001, 0.0001)));
    float2 nSide = (q.x > q.y) ? float2(-sg.x, 0.0) : float2(0.0, -sg.y);
    float2 n = (min(q.x, q.y) > 0.0) ? nCorner : nSide;

    float t = saturate(1.0 - d / bw);
    float2 off = n * (strength * 0.5 * t * t) / sz;   // в долях текстуры

    float4 c = tex2D(input, uv);
    float cr = tex2D(input, saturate(uv + off)).r;
    float cg = tex2D(input, saturate(uv + off * 1.12)).g;
    float cb = tex2D(input, saturate(uv + off * 1.26)).b;

    // цвета с умноженной прозрачностью: обе операции линейные, остаётся только не выйти за альфу
    float3 col = float3(cr, cg, cb);
    float l = dot(col, Luma);
    col = lerp(float3(l, l, l), col, saturation) * brightness;
    float a = c.a;
    col = min(max(col, float3(0.0, 0.0, 0.0)), float3(a, a, a));

    // снаружи прозрачно, на самом краю полпикселя сглаживания
    float cover = saturate(d + 0.5);
    return float4(col, a) * cover;
}
