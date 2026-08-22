<template>
  <section>
    <h2>设置</h2>
    <el-card shadow="never" style="max-width:760px;margin-bottom:16px">
      <template #header>店铺信息 <span class="hint">（显示在用户端顶部，保存后顾客端下次打开页面生效）</span></template>
      <p class="line">店名：
        <el-input v-model="shopName" maxlength="30" style="width:300px" placeholder="如：莱尚网电竞馆" />
      </p>
      <p class="line">客户端欢迎语：
        <el-input v-model="welcomeText" maxlength="60" style="width:420px" placeholder="如：欢迎光临，祝您游戏愉快" />
      </p>
      <p class="line"><el-button type="primary" @click="saveShop">保存店铺信息</el-button></p>
    </el-card>
    <el-card shadow="never" style="max-width:760px;margin-bottom:16px">
      <template #header>收款码 <span class="hint">（顾客端打开页面时加载，更换后顾客端下次打开生效；选任意图片自动裁剪成 300×300）</span></template>
      <div class="qrs">
        <div v-for="q in [{k:'wechat',n:'微信收款码'},{k:'alipay',n:'支付宝收款码'}]" :key="q.k" class="qr">
          <img :src="qrSrc(q.k)">
          <div style="margin:6px 0">{{ q.n }}</div>
          <el-button size="small" @click="qrInputs[q.k].click()">更换图片</el-button>
          <input :ref="el => qrInputs[q.k] = el" type="file" accept="image/*" style="display:none" @change="e => upQr(q.k, e)">
        </div>
      </div>
    </el-card>
    <el-card shadow="never" style="max-width:760px">
      <template #header>系统</template>
      <p class="line">
        <el-switch v-model="autostart" @change="setAuto" style="margin-right:8px" />开机自动启动（吧台主机重启后不用手动开）
      </p>
      <p class="line">服务端口：<b>{{ port }}</b>（在 config.ini 中修改后重启生效，界面上不提供改动）</p>
      <p class="line">
        语音播报：
        <el-button size="small" @click="testSound">🔊 试听播报效果</el-button>
        <span class="hint">（"PC-08 购买商品"）</span>
      </p>
      <p class="line">
        订单提醒：
        <el-button size="small" @click="testNotify">🔔 测试提醒弹窗</el-button>
        <span class="hint">（右下角应弹出"PC-08 呼叫网管"卡片）</span>
      </p>
    </el-card>
  </section>
</template>

<script setup>
import { ref, onMounted } from 'vue'
// ElMessage 由 unplugin-auto-import 自动导入并注入样式（手动 import 会丢 CSS）
import { invoke } from '@tauri-apps/api/core'
import { upload, to300, post, api, PORT, API } from '../api'

const autostart = ref(false), port = PORT
const shopName = ref(''), welcomeText = ref('')
const qrT = ref(Date.now())
const qrInputs = ref({})
const qrSrc = k => `${API}/qrcode/${k}?t=${qrT.value}`

async function upQr(kind, e) {
  const f = e.target.files[0]
  if (!f) return
  try {
    const blob = await to300(f, 'image/png')
    await upload(`/api/admin/qrcode/${kind}`, blob)
    qrT.value = Date.now()
    ElMessage.success('收款码已更新')
  } catch (ex) { ElMessage.error('上传失败：' + ex.message) }
  e.target.value = ''
}
async function setAuto(v) {
  await invoke('set_autostart', { enabled: v })
  ElMessage.success(v ? '已开启开机自启' : '已关闭开机自启')
}
async function testSound() {
  await invoke('test_announce')
}
async function testNotify() {
  await invoke('test_notify')
}
async function saveShop() {
  try {
    await post('/api/admin/shopinfo', { shop_name: shopName.value, welcome: welcomeText.value })
    ElMessage.success('店铺信息已保存')
  } catch (ex) { ElMessage.error('保存失败：' + ex.message) }
}
onMounted(async () => {
  autostart.value = await invoke('get_autostart')
  try {
    const d = await api('/api/admin/shopinfo')
    shopName.value = d.shop_name
    welcomeText.value = d.welcome
  } catch { /* 首次启动用默认 */ }
})
</script>

<style scoped>
h2 { font-size: 17px; margin-bottom: 14px; }
.qrs { display: flex; gap: 40px; }
.qr { text-align: center; font-size: 14px; }
.qr img { width: 150px; height: 150px; border: 1px solid #e5e7eb; border-radius: 8px; }
.hint { color: #999; font-size: 13px; font-weight: normal; }
.line { font-size: 15px; line-height: 2.4; display: flex; align-items: center; }
</style>
