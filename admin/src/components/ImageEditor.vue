<template>
  <div class="editor-mask" @click.self="emit('cancel')">
    <div class="editor">
      <header>
        <span class="t">商品图片</span>
        <button class="close" @click="emit('cancel')">✕</button>
      </header>

      <!-- 文件选择 input：常驻渲染，空态按钮和「重新选图」rePick 共用 -->
      <input ref="fileInput" type="file" accept="image/*" style="display:none" @change="onFile">

      <!-- 无初始图：仅异常兜底（正常流程父组件选完图才打开本组件，不会走到这里） -->
      <div v-if="!initialFile" class="stage empty">
        <button class="pick" @click="fileInput.click()">
          <span class="ico">🖼</span>选择图片文件
        </button>
      </div>
      <!-- 有图：选完图立即显示画布（空白），抠图完成后再填充 -->
      <div v-else class="stage">
        <!-- 预览画布 = 最终保存的 300×300 白底图，所见即所得 -->
        <canvas ref="cv" width="300" height="300" class="cv"
                @mousedown="dragStart" @mousemove="dragMove" @mouseup="dragEnd" @mouseleave="dragEnd" />
      </div>

      <!-- 编辑工具栏 -->
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
            <button @click="rePick">重新选图</button>
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
// 与手机端 mobile/src/components/ImageEditor.vue 同一套交互，
// 差异：管理端没有摄像头，打开组件即弹出文件管理器选图（onMounted 自动 click）。
import { ref, nextTick, onMounted } from 'vue'
import { cutout } from '../bgrem'

const emit = defineEmits(['done', 'cancel'])
// 管理端：由父组件先选好图再打开本组件（initial-file 直接处理），本组件不再自动弹文件选择框
const props = defineProps({ initialFile: { type: File, default: null } })

const sourceImg = ref(null)   // 当前画布显示的图（抠图完成后才有）
const angle = ref(0)          // 连续角度：-45 ~ +45，用于把歪的商品摆正
const flipX = ref(false)
const scale = ref(1)
const fileInput = ref(null)
const cv = ref(null)

onMounted(() => {
  // 父组件已选好图：直接处理。若没有（异常兜底）才弹文件框。
  if (props.initialFile) {
    processFile(props.initialFile)
  } else {
    fileInput.value?.click()
  }
})

async function onFile(e) {
  const f = e.target.files[0]
  if (!f) return
  e.target.value = ''
  processFile(f)
}

// 选完图先显示空白画布（涂白），抠图后台完成后再填充抠好的透明图；失败保留空白并提示
async function processFile(f) {
  render() // 立刻画一张空白 300×300 白底
  try {
    const blob = await cutout(f)
    loadImg(URL.createObjectURL(blob))
  } catch (ex) {
    ElMessage.warning('抠图失败：' + (ex?.message || ex) + '，可重新选图或直接填写其他信息')
  }
}

function rePick() {
  sourceImg.value = null
  angle.value = 0; scale.value = 1; flipX.value = false
  nextTick(() => fileInput.value?.click())
}

function loadImg(url) {
  const img = new Image()
  img.onload = async () => {
    sourceImg.value = img
    await nextTick()
    render()
  }
  // 图片解码失败（文件损坏/HEIC 等非常规格式）：提示换一张
  img.onerror = () => ElMessage.error('图片读取失败，请换一张试试')
  img.src = url
}

// 把 抠图结果 + 角度/翻转/缩放 合成到 300×300 白底画布（预览即成品）
// 即便还没图也先把画布涂白 —— 选图后立即显示空白画布，抠图完成再填充
function render() {
  const c = cv.value
  if (!c) return
  const ctx = c.getContext('2d')
  ctx.fillStyle = '#fff'
  ctx.fillRect(0, 0, 300, 300)
  const img = sourceImg.value
  if (!img) return
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
function dragStart(e) { dragX0 = e.clientX; dragAngle0 = angle.value }
function dragMove(e) {
  if (dragX0 === null) return
  angle.value = Math.max(-45, Math.min(45, dragAngle0 + (e.clientX - dragX0) / 4))
  render()
}
function dragEnd() { dragX0 = null }

// 画布即成品，直接导出 300×300 白底 JPEG
function apply() {
  if (!sourceImg.value) { ElMessage.warning('商品图还没处理好，请稍候或重新选图'); return }
  cv.value.toBlob(b => {
    if (b) emit('done', b)
    else ElMessage.error('图片导出失败，请重试')
  }, 'image/jpeg', 0.9)
}
</script>

<style scoped>
.editor-mask { position: fixed; inset: 0; background: rgba(0,0,0,.5); z-index: 2100; display: flex; align-items: center; justify-content: center; padding: 16px; }
.editor { width: 100%; max-width: 420px; background: #fff; border-radius: 12px; overflow: hidden; }
header { display: flex; align-items: center; justify-content: space-between; padding: 14px 16px; border-bottom: 1px solid #f0f0f0; }
header .t { font-size: 16px; font-weight: bold; }
.close { border: none; background: none; font-size: 16px; color: #909399; cursor: pointer; }
.stage { padding: 16px; min-height: 200px; display: flex; align-items: center; justify-content: center; }
.stage.empty { flex-direction: column; gap: 14px; }
.pick { display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 24px 40px; border: 1px dashed #c0c4cc; border-radius: 10px; background: #fafafa; font-size: 14px; color: #606266; cursor: pointer; }
.pick:hover { background: #ecf5ff; border-color: #409eff; color: #409eff; }
.pick .ico { font-size: 26px; }
.tip { margin: 0; font-size: 12px; color: #a8abb2; text-align: center; }
.cv { width: 300px; height: 300px; border: 1px solid #e4e7ed; border-radius: 8px; cursor: grab; }
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
.nudge button:hover { background: #ecf5ff; border-color: #409eff; color: #409eff; }
.tools .tip { margin-top: 10px; }
footer { padding: 12px 16px 16px; display: flex; gap: 10px; }
footer .el-button { flex: 1; }
</style>
