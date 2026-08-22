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
import { getCategories, addProduct, uploadImage } from '../api'
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
  if (!abbr.value) abbr.value = genAbbr(name.value)
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
    let pic = ''
    if (picBlob.value) {
      const fname = (abbr.value || 'p' + Date.now()) + '.jpg'
      await uploadImage(fname, picBlob.value)
      pic = fname
    }
    await addProduct({
      name: name.value.trim(),
      class: cat.value,
      abbr: abbr.value,
      jhj: jhj.value,
      price: price.value,
      pic,
    })
    // 成功：清空表单，方便连续添加
    name.value = ''; abbr.value = ''; cat.value = ''
    jhj.value = 0; price.value = 0
    picBlob.value = null; picPreview.value = ''
    ElMessage.success('商品已保存')
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
  } catch (e) {
    err.value = e.message
  }
})
</script>

<style scoped>
.page { padding-bottom: 24px; }
header { display: flex; align-items: center; justify-content: space-between; padding: 14px 16px; background: #fff; border-bottom: 1px solid #f0f0f0; }
header .t { font-size: 18px; font-weight: bold; }
.form { padding: 16px; }
.imgbox { width: 100%; aspect-ratio: 1/1; border: 1px dashed #d9d9d9; border-radius: 8px; display: flex; align-items: center; justify-content: center; cursor: pointer; color: #909399; font-size: 15px; text-align: center; background: #fafafa; margin-bottom: 16px; overflow: hidden; }
.imgbox img { width: 100%; height: 100%; object-fit: contain; }
.price-row { display: flex; align-items: center; gap: 8px; width: 100%; }
.sep { color: #909399; font-size: 13px; }
</style>
