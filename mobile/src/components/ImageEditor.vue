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
        <div v-else class="preview">
          <img :src="previewUrl" class="pimg">
        </div>
      </div>

      <!-- 编辑工具栏（有图且非抠图中） -->
      <div v-if="sourceImg && !cutting" class="tools">
        <button @click="rotate">⟳ 旋转</button>
        <button @click="flip">⇋ 翻转</button>
        <label>缩放 <input type="range" v-model.number="scale" min="0.5" max="2" step="0.05"></label>
      </div>

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
import { ref } from 'vue'
import { cutout } from '../bgrem'

const emit = defineEmits(['done', 'cancel'])

const sourceImg = ref(null)   // 抠图后的透明图（Image 元素）
const previewUrl = ref('')
const cutting = ref(false)
const cutProgress = ref(0)
const rotateDeg = ref(0)      // 0/90/180/270
const flipX = ref(false)
const scale = ref(1)
const fileInput = ref(null)
const cameraInput = ref(null)
const rawFile = ref(null)

async function onFile(e) {
  const f = e.target.files[0]
  if (!f) return
  e.target.value = ''
  rawFile.value = f
  cutting.value = true
  cutProgress.value = 0
  try {
    const blob = await cutout(f, p => { cutProgress.value = p })
    const url = URL.createObjectURL(blob)
    const img = new Image()
    img.onload = () => {
      sourceImg.value = img
      previewUrl.value = url
      cutting.value = false
    }
    img.src = url
  } catch {
    // 抠图失败回退：直接用原图，仅旋转/缩放
    const url = URL.createObjectURL(f)
    const img = new Image()
    img.onload = () => {
      sourceImg.value = img
      previewUrl.value = url
      cutting.value = false
      ElMessage.warning('抠图失败，已用原图')
    }
    img.src = url
  }
}

function rotate() { rotateDeg.value = (rotateDeg.value + 90) % 360 }
function flip() { flipX.value = !flipX.value }

// 合成 300×300 白底 JPEG 并返回 Blob
function apply() {
  const img = sourceImg.value
  const c = document.createElement('canvas')
  c.width = c.height = 300
  const ctx = c.getContext('2d')
  // 白底
  ctx.fillStyle = '#fff'
  ctx.fillRect(0, 0, 300, 300)

  // 计算旋转后尺寸（90/270 交换宽高）
  const rad = rotateDeg.value * Math.PI / 180
  const isSwap = rotateDeg.value % 180 !== 0
  const iw = isSwap ? img.height : img.width
  const ih = isSwap ? img.width : img.height
  const s = Math.min(290 / iw, 290 / ih) * scale.value

  ctx.save()
  ctx.translate(150, 150)
  ctx.rotate(rad)
  if (flipX.value) ctx.scale(-1, 1)
  ctx.drawImage(img, -iw * s / 2, -ih * s / 2, iw * s, ih * s)
  ctx.restore()

  c.toBlob(b => {
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
.tip { margin: 0; font-size: 12px; color: #a8abb2; }
.preview { display: flex; align-items: center; justify-content: center; }
.pimg { max-width: 100%; max-height: 340px; object-fit: contain; background: repeating-conic-gradient(#eee 0% 25%, #fff 0% 50%) 50% / 16px 16px; }
.cutting { text-align: center; color: #606266; }
.cutting p { margin-top: 12px; font-size: 13px; }
.tools { display: flex; align-items: center; gap: 10px; padding: 0 16px 12px; }
.tools button { border: 1px solid #d9d9d9; background: #fff; border-radius: 6px; padding: 6px 12px; cursor: pointer; font-size: 13px; }
.tools label { font-size: 13px; color: #606266; display: flex; align-items: center; gap: 6px; }
footer { padding: 12px 16px 16px; display: flex; gap: 10px; }
footer .el-button { flex: 1; }
</style>
