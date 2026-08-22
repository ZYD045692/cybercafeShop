<template>
  <div class="shell">
    <AddProduct />
  </div>
</template>

<script setup>
import AddProduct from './views/AddProduct.vue'
</script>

<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
body { background: #f5f6f8; font-family: "Microsoft YaHei", sans-serif; }
.shell { max-width: 480px; margin: 0 auto; min-height: 100vh; background: #fff; }
</style>
