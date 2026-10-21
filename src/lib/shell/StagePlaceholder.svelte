<script lang="ts">
  let {
    title,
    stage = "M0",
    text,
  }: { title: string; stage?: string; text: string } = $props();
</script>

<section class="placeholder glass liquid-edge">
  <p class="eyebrow">{stage}</p>
  <h1>{title}</h1>
  <p>{text}</p>
</section>

<style>
  .placeholder {
    min-height: 320px;
    border-radius: 20px;
    padding: 32px;
  }

  .eyebrow {
    margin: 0 0 8px;
    color: var(--muted-foreground);
    font-size: 10px;
    font-weight: 650;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  h1 {
    margin: 0 0 10px;
    font-size: 26px;
    font-weight: 650;
    letter-spacing: -0.02em;
  }

  p {
    max-width: 640px;
    margin: 0;
    color: var(--muted-foreground);
    font-size: 13px;
    line-height: 1.5;
  }
</style>
