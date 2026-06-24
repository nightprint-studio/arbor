<script lang="ts">
  /** Transient status toast for launcher actions (open / stop / update). */
  interface Props { msg: string; color: string; }
  let { msg, color }: Props = $props();
</script>

<div class="cv-toast">
  <span class="dot" style="background:{color};box-shadow:0 0 9px {color}"></span>
  <span class="msg">{msg}</span>
</div>

<style>
  .cv-toast {
    position: absolute; left: 50%; top: 120px; transform: translateX(-50%);
    display: flex; align-items: center; gap: 9px; z-index: 8; white-space: nowrap;
    background: rgba(14, 18, 26, 0.93); border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 12px; padding: 10px 16px;
    box-shadow: 0 16px 40px -12px rgba(0, 0, 0, 0.8);
  }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex: none; display: inline-block; }
  .msg { font-size: 13px; color: #dfe5ee; font-weight: 500; }
</style>
