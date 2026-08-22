<template>
  <div class="page">
    <header>
      <span class="t">添加商品</span>
    </header>

    <div class="form">
      <!-- 图片：点击进入抠图编辑器 -->
      <div class="imgbox" @click="editorOpen = true">
        <img v-if="picBlob" :src="picPreview">
        <span v-else>选择商品图片<br><small>可抠图 / 旋转</small></span>
      </div>

      <el-form label-position="top" size="large">
        <el-form-item label="名称">
          <el-input v-model="name" placeholder="如 百事可乐500ml" @input="onName" />
        </el-form-item>
        <el-form-item label="分类">
          <el-select v-model="cat" placeholder="选择分类" style="width:100%">
            <el-option v-for="c in cats" :key="c.name" :label="c.name" :value="c.name" />
          </el-select>
        </el-form-item>
        <el-form-item label="缩拼">
          <el-input v-model="abbr" placeholder="自动生成，可手改" />
        </el-form-item>
        <el-form-item label="进价 / 售价">
          <div class="price-row">
            <el-input-number v-model="jhj" :min="0" :precision="1" :step="0.5" controls-position="right" style="width:48%" />
            <span class="sep">进</span>
            <el-input-number v-model="price" :min="0" :precision="1" :step="0.5" controls-position="right" style="width:48%" />
            <span class="sep">售</span>
          </div>
        </el-form-item>
      </el-form>

      <el-alert v-if="err" :title="err" type="error" :closable="false" style="margin-bottom:12px" />

      <el-button type="primary" size="large" style="width:100%" :loading="saving" @click="save">保存商品</el-button>
    </div>

    <!-- 抠图编辑器 -->
    <ImageEditor v-if="editorOpen" @done="onImageDone" @cancel="editorOpen = false" />
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { getCategories, addProduct, uploadProductImage } from '../api'
import { genAbbr } from '../pinyin'
import ImageEditor from '../components/ImageEditor.vue'

const cats = ref([])
const name = ref(''), abbr = ref(''), cat = ref('')
const jhj = ref(0), price = ref(0)
const picBlob = ref(null), picPreview = ref('')
const editorOpen = ref(false)
const saving = ref(false)
const err = ref('')

function onName() {
  // 与管理端一致：名称一变就无条件重新生成缩拼（手填的缩拼在改名时会跟随刷新）
  abbr.value = genAbbr(name.value)
}

function onImageDone(blob) {
  picBlob.value = blob
  picPreview.value = URL.createObjectURL(blob)
  editorOpen.value = false
}

async function save() {
  err.value = ''
  // 校验顺序：先名称 → 再缩拼 → 最后售价
  if (!name.value.trim()) { err.value = '请填写商品名称'; return }
  if (!abbr.value.trim()) { err.value = '该商品名生成不出缩拼，请手动填写'; return }
  if (price.value <= 0) { err.value = '售价不能为 0'; return }

  saving.value = true
  try {
    // 顺序：先建商品（服务端会唯一化缩拼），再按商品 id 传图，
    // 图片文件名由服务端按最终缩拼生成——缩拼冲突时不会覆盖已有商品的图
    const { id } = await addProduct({
      name: name.value.trim(),
      class: cat.value,
      abbr: abbr.value,
      jhj: jhj.value,
      price: price.value,
      pic: '',
    })
    let imgFailed = null
    if (picBlob.value) {
      try {
        await uploadProductImage(id, picBlob.value)
      } catch (e) {
        imgFailed = e.message
      }
    }
    // 商品已建好（即使图片失败）：清空表单，方便连续添加；
    // 也避免用户因图片报错重复点保存 → 重复建出 xx_1 商品
    name.value = ''; abbr.value = ''; cat.value = cats.value[0]?.name || ''
    jhj.value = 0; price.value = 0
    picBlob.value = null; picPreview.value = ''
    if (imgFailed) {
      ElMessage.warning('商品已保存，但图片上传失败：' + imgFailed + '，可在管理端补传')
    } else {
      ElMessage.success('商品已保存')
    }
  } catch (e) {
    err.value = e.message
  } finally {
    saving.value = false
  }
}

onMounted(async () => {
  try {
    const d = await getCategories()
    cats.value = d.categories
    if (cats.value.length) cat.value = cats.value[0].name
  } catch {
    ElMessage.error('无法连接主机。')
  }
})
</script>

<style scoped>
.page { padding-bottom: 24px; }
header { display: flex; align-items: center; justify-content: space-between; padding: 14px 16px; background: #fff; border-bottom: 1px solid #f0f0f0; }
header .t { font-size: 18px; font-weight: bold; }
.form { padding: 16px; }
/* 行与行之间：Element large 默认 22px，这里收紧到 8px */
.form :deep(.el-form-item) { margin-bottom: 8px; }
.imgbox { width: 80%; margin: 0 auto 16px; aspect-ratio: 1/1; border: 1px dashed #d9d9d9; border-radius: 8px; display: flex; align-items: center; justify-content: center; cursor: pointer; color: #909399; font-size: 15px; text-align: center; background: #fafafa; overflow: hidden; }
.imgbox img { width: 100%; height: 100%; object-fit: contain; }
.price-row { display: flex; align-items: center; gap: 8px; width: 100%; }
.sep { color: #909399; font-size: 13px; }
</style>
