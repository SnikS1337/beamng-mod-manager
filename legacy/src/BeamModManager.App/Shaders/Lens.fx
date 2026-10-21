// Линза капли для подписей (2026-10-18). У Glass.fx выборки у кромки сдвигаются внутрь: на ровном фоне это «жидкий» край,
// а подпись или значок у кромки размазывались к краю полосами, и половина капли без подписи оставалась ровной — «радуга
// обрывается на середине». Здесь как у выпуклой линзы: у кромки видно то, что лежит чуть снаружи капли, сжатое к краю.
// Сдвиг наружу растёт к кромке монотонно (сжатие, без складок и полос), каналы R, G, B — с множителями 1.0 / 1.12 / 1.26.
// Картинка на входе больше капли на поле со всех сторон (его рисует Droplet), форма капли — внутри.
// Сборка: fxc /T ps_2_0 /E main (цель CompileGlassShader в BeamModManager.App.csproj).

sampler2D input : register(s0);

float2 size : register(c0);      // размер картинки с полем
float4 corner : register(c1);    // x — радиус капли; yz — половина размера капли минус радиус
float invLens : register(c2);    // 1 / ширина пояса у кромки, где идёт сжатие
float push : register(c3);       // насколько снаружи берётся точка у самой кромки
float under : register(c4);      // яркость того, что под подписями: у радужной каймы недостающий канал — цвет фона
float4 edge : register(c8);      // границы выборок в долях (на точку от края картинки: видеокарта подмешивает прозрачное)
float4 geo : register(c9);       // xy — 1 / размер картинки, zw — половина размера картинки

float4 main(float2 uv : TEXCOORD) : COLOR
{
    float2 p = uv * size - geo.zw;
    float2 sg = sign(p);
    float2 q = abs(p) - corner.yz;
    float d = -(length(max(q, float2(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - corner.x);

    // нормаль наружу: в углу — от центра скругления, на прямом участке — по оси ближнего края
    float2 nCorner = sg * normalize(max(q, float2(0.0001, 0.0001)));
    float2 nSide = (q.x > q.y) ? float2(sg.x, 0.0) : float2(0.0, sg.y);
    float2 n = (min(q.x, q.y) > 0.0) ? nCorner : nSide;

    float t = saturate(1.0 - d * invLens);
    float2 off = n * (push * t * t) * geo.xy;

    float4 r = tex2D(input, clamp(uv + off, edge.xy, edge.zw));
    float4 g = tex2D(input, clamp(uv + off * 1.12, edge.xy, edge.zw));
    float4 b = tex2D(input, clamp(uv + off * 1.26, edge.xy, edge.zw));

    // цвета с умноженной прозрачностью: канал, который мимо подписи, берёт цвет фона — так кайма цветная и на светлом
    float a = max(max(r.a, g.a), b.a);
    float3 col = float3(r.r, g.g, b.b) + (a - float3(r.a, g.a, b.a)) * under;

    float cover = saturate(d + 0.5);
    return float4(col, a) * cover;
}
