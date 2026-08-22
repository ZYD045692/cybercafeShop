<template>
  <div class="editor-mask" @click.self="emit('cancel')">
    <div class="editor">
      <header>
        <span class="t">商品图片</span>
        <button class="close" @click="emit('cancel')">✕</button>
      </header>

      <!-- 未选图：拍照 或 从相册选择 -->
      <div v-if="!sourceImg" class="stage empty">
        <!-- capture="environment"：手机浏览器直接调起后置摄像头；不带 capture 的走相册/文件 -->
        <input ref="cameraInput" type="file" accept="image/*" capture="environment" style="display:none" @change="onFile">
        <input ref="fileInput" type="file" accept="image/*" style="display:none" @change="onFile">
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

      <!-- 已选图：抠图进度 / 编辑预览 -->
      <div v-else class="stage">
        <div v-if="cutting" class="cutting">
          <el-progress type="circle" :percentage="cutProgress" :width="90" />
          <p>正在抠图，首次加载模型需稍等…</p>
        </div>
        <!-- 预览画布 = 最终保存的 300×300 白底图，所见即所得 -->
        <canvas v-else ref="cv" width="300" height="300" class="cv"
                @mousedown="dragStart" @mousemove="dragMove" @mouseup="dragEnd" @mouseleave="dragEnd"
                @touchstart.passive="dragStart" @touchmove.prevent="dragMove" @touchend="dragEnd" />
      </div>

      <!-- 编辑工具栏（有图且非抠图中） -->
      <template v-if="sourceImg && !cutting">
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
            <button @click="resetAll">重置</button>
          </div>
          <p class="tip">也可以直接在图片上左右拖动来旋转</p>
        </div>
      </template>

      <footer v-if="sourceImg && !cutting">
        <el-button @click="emit('cancel')">取消</el-button>
        <el-button type="primary" @click="apply">应用</el-button>
      </footer>
      <footer v-if="cutting">
        <el-button @click="emit('cancel')">取消</el-button>
      </footer>
    </div>
  </div>
</template>

<script setup>
import { ref, nextTick } from 'vue'
import { cutout } from '../bgrem'

const emit = defineEmits(['done', 'cancel'])

const sourceImg = ref(null)   // 抠图后的透明图（Image 元素）
const cutting = ref(false)
const cutProgress = ref(0)
const angle = ref(0)          // 连续角度：-45 ~ +45，用于把拍歪的商品摆正
const flipX = ref(false)
const scale = ref(1)
const fileInput = ref(null)
const cameraInput = ref(null)
const cv = ref(null)

async function onFile(e) {
  const f = e.target.files[0]
  if (!f) return
  e.target.value = ''
  cutting.value = true
  cutProgress.value = 0
  try {
    const blob = await cutout(f, p => { cutProgress.value = p })
    loadImg(URL.createObjectURL(blob))
  } catch (ex) {
    // 抠图失败回退：直接用原图，仅旋转/缩放
    console.warn('抠图失败，回退原图：', ex)
    loadImg(URL.createObjectURL(f))
    ElMessage.warning('抠图失败，已用原图')
  }
}

function loadImg(url) {
  const img = new Image()
  img.onload = async () => {
    sourceImg.value = img
    cutting.value = false
    await nextTick()
    render()
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
  cv.value.toBlob(b => {
    if (b) emit('done', b)
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
.cutting { text-align: center; color: #606266; }
.cutting p { margin-top: 12px; font-size: 13px; }
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
