<template>
  <div class="editor-mask" @click.self="emit('cancel')">
    <div class="editor">
      <header>
        <span class="t">商品图片</span>
        <button class="close" @click="emit('cancel')">✕</button>
      </header>

      <!-- 未选图：拍照 或 从相册选择 -->
      <div v-if="!picked" class="stage empty">
        <!-- capture="environment"：手机浏览器直接调起后置摄像头；不带 capture 的走相册/文件 -->
        <input ref="cameraInput" type="file" accept="image/*" capture="environment" style="display:none" @change="onFile($event,'camera')">
        <input ref="fileInput" type="file" accept="image/*" style="display:none" @change="onFile($event,'gallery')">
        <div class="pick-btns">
          <button class="pick" @click="cameraInput.click()">
            <span class="ico">📷</span>拍照
          </button>
          <button class="pick" @click="fileInput.click()">
            <span class="ico">🖼</span>从相册选择
          </button>
        </div>
        <p class="tip">建议把商品放在干净背景上拍摄，抠图效果更好</p>
      </div>
      <!-- 已选图：画布（选完立即空白，抠图完成后再填充；失败保持空白可重新选择） -->
      <div v-else class="stage">
        <div class="cv-wrap">
          <!-- 预览画布 = 最终保存的 300×300 白底图，所见即所得 -->
          <canvas ref="cv" width="300" height="300" class="cv"
                  @mousedown="dragStart" @mousemove="dragMove" @mouseup="dragEnd" @mouseleave="dragEnd"
                  @touchstart.passive="dragStart" @touchmove.prevent="dragMove" @touchend="dragEnd" />
          <!-- 处理中：下载模型（仅首次）+ 处理图片 两个独立圆形进度圈，圈内各自 0→100% -->
          <div v-if="downloading || processing" class="process">
            <div v-if="downloading" class="proc-row">
              <p>正在下载模型…</p>
              <el-progress type="circle" :percentage="Math.round(downloadProgress)" :width="90" :stroke-width="8" />
            </div>
            <div v-if="processing" class="proc-row">
              <p>正在处理图片…</p>
              <el-progress type="circle" :percentage="Math.round(processProgress)" :width="90" :stroke-width="8" />
            </div>
          </div>
        </div>
      </div>

      <!-- 编辑工具栏（有抠好的图才出现） -->
      <template v-if="sourceImg">
        <div class="tools">
          <div class="row">
            <div class="row-head">
              <label>旋转角度（摆正）</label>
              <span class="val">{{ angle.toFixed(1) }}°</span>
            </div>
            <input type="range" v-model.number="angle" min="-45" max="45" step="0.5" @input="render">
            <div class="nudge">
              <button @click="nudge(-1)">−1°</button>
              <button @click="nudge(-0.5)">−0.5°</button>
              <button @click="nudge(0.5)">+0.5°</button>
              <button @click="nudge(1)">+1°</button>
            </div>
          </div>
          <div class="row">
            <div class="row-head">
              <label>缩放</label>
              <span class="val">{{ Math.round(scale * 100) }}%</span>
            </div>
            <input type="range" v-model.number="scale" min="0.5" max="2" step="0.05" @input="render">
          </div>
          <div class="nudge">
            <button @click="flipX = !flipX; render()">⇋ 水平翻转</button>
            <button @click="rePick">{{ enterFrom === 'camera' ? '重新拍照' : '重新选择' }}</button>
            <button @click="resetAll">重置</button>
          </div>
          <p class="tip">也可以直接在图片上左右拖动来旋转</p>
        </div>
      </template>

      <footer v-if="sourceImg">
        <el-button @click="emit('cancel')">取消</el-button>
        <el-button type="primary" plain @click="apply">应用</el-button>
      </footer>
    </div>
  </div>
</template>

<script setup>
import { ref, nextTick, onUnmounted } from 'vue'
import { cutout, disposeBgrem } from '../bgrem'

const emit = defineEmits(['done', 'cancel'])

const sourceImg = ref(null)   // 抠图后的透明图（Image 元素）
const picked = ref(false)     // 已选图（画布常驻显示；未选图才显示拍照/相册入口）
const downloading = ref(false)   // 首次无缓存：正在下载模型（显示下载进度条）
const downloadProgress = ref(0)  // 下载进度 0~100（真实 fetch 进度）
const processing = ref(false)    // 处理图片中（显示处理进度条）
const processProgress = ref(0)   // 处理图片假进度 0~100
const angle = ref(0)          // 连续角度：-45 ~ +45，用于把拍歪的商品摆正
const flipX = ref(false)
const scale = ref(1)
const fileInput = ref(null)
const cameraInput = ref(null)
const cv = ref(null)
const enterFrom = ref('gallery') // 记录本次进图来源：camera=拍照 / gallery=相册，重选时据此弹对应入口

let processTimer = null
// 处理图片阶段假进度：抠图库无真实进度回调，用定时器平滑爬升到 90% 封顶，真正完成时跳到 100。
// 下载模型阶段用 bgrem.js 报的真实 fetch 进度，不在这里模拟。
function startProcessProgress() {
  processing.value = true
  processProgress.value = 0
  clearInterval(processTimer)
  processTimer = setInterval(() => {
    if (processProgress.value < 90) processProgress.value += 1 + Math.random() * 2
  }, 150)
}
function stopProcessProgress() {
  clearInterval(processTimer)
  processProgress.value = 100
  processing.value = false
}

// 卸载时释放：停掉假进度定时器 + 释放进行中的模型 session（避免管理端常驻时内存泄漏）
onUnmounted(() => {
  clearInterval(processTimer)
  disposeBgrem()
})

async function onFile(e, src = 'gallery') {
  const f = e.target.files[0]
  if (!f) return
  e.target.value = ''
  enterFrom.value = src
  picked.value = true
  await nextTick() // 画布挂载后再涂白
  render() // 立刻画一张空白 300×300 白底，抠图完成后再填充
  downloading.value = false
  downloadProgress.value = 0
  try {
    const blob = await cutout(f, ev => {
      if (ev.stage === 'download') {
        // 仅首次无缓存才走到这里：显示「下载模型」真实进度
        downloading.value = true
        downloadProgress.value = ev.percent ?? 0
      } else if (ev.stage === 'image') {
        // 模型就位，开始处理图片
        downloading.value = false
        downloadProgress.value = 100
        startProcessProgress()
      }
    })
    stopProcessProgress()
    loadImg(URL.createObjectURL(blob))
  } catch (ex) {
    // 抠图失败：回到选图页重新选择（画布不留空白卡死）
    stopProcessProgress()
    downloading.value = false
    picked.value = false
    ElMessage.warning('抠图失败：' + (ex?.message || ex) + '，请重新选择')
  }
}

// 重新选图：按本次进图来源，弹对应入口（拍照→重新拍照，相册→重新选择）；picked 保持 true，画布持续显示
function rePick() {
  sourceImg.value = null
  angle.value = 0; scale.value = 1; flipX.value = false
  nextTick(() => (enterFrom.value === 'camera' ? cameraInput : fileInput).value?.click())
}

function loadImg(url) {
  const img = new Image()
  img.onload = async () => {
    sourceImg.value = img
    await nextTick()
    render()
  }
  // 图片解码失败（文件损坏/HEIC 等非常规格式）：回到选图页重新选择
  img.onerror = () => {
    picked.value = false
    ElMessage.error('图片读取失败，请换一张试试')
  }
  img.src = url
}

// 把 抠图结果 + 角度/翻转/缩放 合成到 300×300 白底画布（预览即成品）
function render() {
  const img = sourceImg.value
  const c = cv.value
  if (!img || !c) return
  const ctx = c.getContext('2d')
  ctx.fillStyle = '#fff'
  ctx.fillRect(0, 0, 300, 300)
  const rad = angle.value * Math.PI / 180
  // 任意角度都适用：按原图宽高比适配进 290 内框，再乘用户缩放
  const s = Math.min(290 / img.width, 290 / img.height) * scale.value
  ctx.save()
  ctx.translate(150, 150)
  ctx.rotate(rad)
  if (flipX.value) ctx.scale(-1, 1)
  ctx.drawImage(img, -img.width * s / 2, -img.height * s / 2, img.width * s, img.height * s)
  ctx.restore()
}

function nudge(d) {
  angle.value = Math.max(-45, Math.min(45, angle.value + d))
  render()
}
function resetAll() {
  angle.value = 0; scale.value = 1; flipX.value = false
  render()
}

// 图片上直接左右拖动 = 旋转（每 4px ≈ 1°）
let dragX0 = null, dragAngle0 = 0
function evtX(e) { return e.touches ? e.touches[0].clientX : e.clientX }
function dragStart(e) { dragX0 = evtX(e); dragAngle0 = angle.value }
function dragMove(e) {
  if (dragX0 === null) return
  angle.value = Math.max(-45, Math.min(45, dragAngle0 + (evtX(e) - dragX0) / 4))
  render()
}
function dragEnd() { dragX0 = null }

// 画布即成品，直接导出 300×300 白底 JPEG
function apply() {
  if (!sourceImg.value) { ElMessage.warning('商品图还没处理好，请稍候或重新选择'); return }
  cv.value.toBlob(b => {
    if (b) emit('done', b)
    else ElMessage.error('图片导出失败，请重试')
  }, 'image/jpeg', 0.9)
}
</script>

<style scoped>
.editor-mask { position: fixed; inset: 0; background: rgba(0,0,0,.5); z-index: 100; display: flex; align-items: center; justify-content: center; padding: 16px; }
.editor { width: 100%; max-width: 420px; background: #fff; border-radius: 12px; overflow: hidden; }
header { display: flex; align-items: center; justify-content: space-between; padding: 14px 16px; border-bottom: 1px solid #f0f0f0; }
header .t { font-size: 16px; font-weight: bold; }
.close { border: none; background: none; font-size: 16px; color: #909399; cursor: pointer; }
.stage { padding: 16px; min-height: 200px; display: flex; align-items: center; justify-content: center; }
.stage.empty { flex-direction: column; gap: 14px; }
.pick-btns { display: flex; gap: 12px; width: 100%; }
.pick { flex: 1; display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 20px 0; border: 1px dashed #c0c4cc; border-radius: 10px; background: #fafafa; font-size: 14px; color: #606266; cursor: pointer; }
.pick:active { background: #ecf5ff; border-color: #409eff; color: #409eff; }
.pick .ico { font-size: 26px; }
.tip { margin: 0; font-size: 12px; color: #a8abb2; text-align: center; }
.cv { width: 300px; height: 300px; border: 1px solid #e4e7ed; border-radius: 8px; touch-action: none; }
.cv-wrap { position: relative; }
.process { position: absolute; inset: 0; background: rgba(255,255,255,.92); border-radius: 8px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px; }
.proc-row { display: flex; flex-direction: column; align-items: center; gap: 6px; }
.process p { margin: 0; font-size: 13px; color: #606266; }
.tools { padding: 0 16px 12px; }
.row { margin-bottom: 12px; }
.row-head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 4px; }
.row-head label { font-size: 14px; color: #303133; }
.row-head .val { font-size: 13px; color: #909399; font-variant-numeric: tabular-nums; }
.tools input[type=range] { width: 100%; height: 28px; }
.nudge { display: flex; gap: 8px; margin-top: 8px; }
.nudge button { flex: 1; padding: 8px 0; border: 1px solid #dcdfe6; border-radius: 6px; background: #fff; font-size: 13px; color: #606266; cursor: pointer; }
.nudge button:active { background: #ecf5ff; border-color: #409eff; color: #409eff; }
.tools .tip { margin-top: 10px; }
footer { padding: 12px 16px 16px; display: flex; gap: 10px; }
footer .el-button { flex: 1; }
</style>
