<template>
  <section>
    <div class="bar">
      <el-radio-group v-model="cls" class="cats" size="default">
        <el-radio-button value="">全部</el-radio-button>
        <el-radio-button v-for="c in cats.filter(x => x.name !== '全部商品')" :key="c.name" :value="c.name">{{ c.name }}</el-radio-button>
      </el-radio-group>
      <el-radio-group v-model="stateFilter" size="default">
        <el-radio-button value="all">全部</el-radio-button>
        <el-radio-button value="on">正在销售</el-radio-button>
        <el-radio-button value="off">停止销售</el-radio-button>
      </el-radio-group>
      <el-input v-model="kw" placeholder="输入商品名或缩拼搜索" clearable style="width:200px" />
      <el-button type="primary" plain @click="edit(null)">＋ 添加商品</el-button>
      <el-button @click="catDlg = true">分类管理</el-button>
    </div>
    <div class="mgrid" v-loading="loading">
      <div v-for="p in filtered" :key="p.id" class="mcard" :class="{ off: !p.state }">
        <el-switch :model-value="!!p.state" class="sw" @change="toggle(p)"
          :title="p.state ? '在售，点击下架' : '已下架，点击上架'" />
        <div class="pic" @click="edit(p)">
          <img v-if="p.pic" :src="imgUrl(p.pic) + '?t=' + imgT" loading="lazy">
          <span v-else style="color:#cbd5e1;font-size:40px">🥤</span>
        </div>
        <div class="pn">{{ p.name }}</div>
        <div class="pp"><span>进价 ¥{{ p.jhj.toFixed(1) }}</span><b>¥{{ p.price.toFixed(1) }}</b></div>
      </div>
      <div v-if="!filtered.length && !loading" class="mempty">{{ emptyText }}</div>
    </div>

    <!-- 编辑/新增弹窗；destroy-on-close：关闭即销毁内容，确保 ImageEditor 卸载触发 onUnmounted 释放模型 session -->
    <el-dialog v-model="editing" :title="form.id ? '编辑商品' : '添加商品'" width="520px" destroy-on-close>
      <el-form label-width="60px">
        <div class="picline">
          <el-form-item label="图片">
            <div class="picbox" @click="fileInput.click()">
              <img v-if="preview" :src="preview">
              <span v-else>点击选择图片</span>
            </div>
            <input ref="fileInput" type="file" accept="image/*" style="display:none" @change="onFile">
          </el-form-item>
          <el-form-item v-if="qrImg" label="扫码添加商品" label-width="100px">
            <img class="qrimg" :src="qrImg" :title="mobileUrl">
          </el-form-item>
        </div>
        <el-form-item label="名称">
          <el-input v-model="form.name" placeholder="如 百事可乐500ml" />
        </el-form-item>
        <el-form-item label="分类">
          <el-select v-model="form.class" placeholder="选择分类" style="width:100%">
            <el-option v-for="c in cats" :key="c.name" :label="c.name" :value="c.name" />
          </el-select>
        </el-form-item>
        <el-form-item label="缩拼">
          <el-input v-model="form.abbr" placeholder="小写字母，如 bskl" :disabled="!!form.id" />
        </el-form-item>
        <el-form-item label="价格">
          <el-input-number v-model="form.jhj" :min="0" :precision="1" :step="0.5" />
          <span style="margin:0 8px;color:#909399">进价</span>
          <el-input-number v-model="form.price" :min="0" :precision="1" :step="0.5" />
          <span style="margin:0 8px;color:#909399">售价</span>
        </el-form-item>
      </el-form>
      <el-alert v-if="err" :title="err" type="error" :closable="false" style="margin-bottom:10px" />
      <template #footer>
        <el-button type="primary" plain @click="save">保存</el-button>
        <el-button @click="editing = false">取消</el-button>
        <el-popconfirm v-if="form.id" title="确定删除该商品？" @confirm="del">
          <template #reference>
            <el-button type="danger" plain style="float:left">删除</el-button>
          </template>
        </el-popconfirm>
      </template>
    </el-dialog>

    <!-- 分类管理弹窗；destroy-on-close：关闭即销毁内容 -->
    <el-dialog v-model="catDlg" title="分类管理" width="400px" destroy-on-close>
      <div v-for="c in cats" :key="c.name" class="catrow">
        <template v-if="renaming === c.name">
          <el-input v-model="renameTo" size="small" style="flex:1" />
          <el-button size="small" type="primary" @click="doRename(c.name)">确定</el-button>
        </template>
        <template v-else>
          <span style="flex:1">{{ c.name }}</span>
          <el-button size="small" link type="primary" @click="renaming = c.name; renameTo = c.name">改名</el-button>
          <el-popconfirm title="确定删除该分类？（分类下有商品时不可删）" @confirm="delCat(c.name)">
            <template #reference>
              <el-button size="small" link type="danger">删除</el-button>
            </template>
          </el-popconfirm>
        </template>
      </div>
      <div class="catrow">
        <el-input v-model="newCat" placeholder="新分类名" size="small" style="flex:1" />
        <el-button size="small" type="success" @click="addCat">添加</el-button>
      </div>
      <el-alert v-if="catErr" :title="catErr" type="error" :closable="false" style="margin-top:10px" />
    </el-dialog>

    <!-- 抠图编辑器：先选图，选完再进编辑器（initial-file 直接处理，不再弹选择框） -->
    <ImageEditor v-if="editorOpen" :initial-file="pendingFile" @done="onImageDone" @cancel="editorOpen = false" />
  </section>
</template>

<script setup>
import { ref, computed, onMounted, watch, onBeforeUnmount, defineAsyncComponent } from 'vue'
import { pinyin } from 'pinyin-pro'
import QRCode from 'qrcode'
import { api, post, del as delApi, upload, imgUrl, PORT } from '../api'
// 抠图编辑器懒加载：其内部会连带加载 @planby-tech/rmbg-webgpu（含 onnxruntime-web），
// 拆成异步 chunk 后不进主 bundle，首次点开才加载，降低管理端常驻内存基线
const ImageEditor = defineAsyncComponent(() => import('../components/ImageEditor.vue'))

const products = ref([]), cats = ref([]), loading = ref(true)
const kw = ref(''), cls = ref(''), stateFilter = ref('all'), imgT = ref(Date.now())
const editing = ref(false), form = ref({}), preview = ref(''), picBlob = ref(null), err = ref('')
let previewBlobUrl = '' // 当前 preview 指向的 blob URL（imgUrl 的 http 图不算），换图/卸载时 revoke 防内存泄漏
function revokePreview() {
  if (previewBlobUrl) { URL.revokeObjectURL(previewBlobUrl); previewBlobUrl = '' }
}
const catDlg = ref(false), newCat = ref(''), catErr = ref(''), renaming = ref(''), renameTo = ref('')
// 抠图编辑器：点图片框 → 直接弹系统文件选择器 → 选完图再打开编辑器（initial-file 传入），done 拿到 300×300 JPEG blob
const editorOpen = ref(false)
const fileInput = ref(null), pendingFile = ref(null)
function onFile(e) {
  const f = e.target.files[0]
  if (!f) return
  e.target.value = ''
  pendingFile.value = f
  editorOpen.value = true
}
function onImageDone(blob) {
  revokePreview() // 换新图先释放上一张的 blob URL
  picBlob.value = blob
  previewBlobUrl = URL.createObjectURL(blob)
  preview.value = previewBlobUrl
  editorOpen.value = false
  pendingFile.value = null
}
// 手机端二维码：内容为本机局域网 IPv4 + 端口的 /m/ 页面地址，手机扫码直接打开
const qrImg = ref(''), mobileUrl = ref('')
async function loadQr() {
  try {
    const h = await api('/api/admin/hostinfo')
    mobileUrl.value = `http://${h.lan_ip}:${PORT}/m/`
    qrImg.value = await QRCode.toDataURL(mobileUrl.value, { width: 200, margin: 1 })
  } catch { /* 拿不到局域网 IP 时只隐藏二维码，不影响弹窗其它功能 */ }
}

const filtered = computed(() => {
  // 搜索词先去空格 + 转小写：只输空格当作没搜（否则搜不到），名称/缩拼都统一用小写匹配（大小写不敏感）
  const q = kw.value.trim().toLowerCase()
  return products.value.filter(p =>
    (!cls.value || p.class === cls.value) &&
    (stateFilter.value === 'all' || (stateFilter.value === 'on' ? !!p.state : !p.state)) &&
    (!q || p.name.toLowerCase().includes(q) || p.abbr.includes(q)))
})

// 空状态提示：类别照打（「全部」→「全部商品」，具体分类 →「XX的商品」）；
// 状态只有「正在销售/停止销售」才打，「全部」不打
const emptyText = computed(() => {
  const cat = (cls.value || '全部') + '商品'
  const st = { all: '', on: '正在销售', off: '停止销售' }[stateFilter.value] || ''
  return `${cat}${st}中没有符合条件的商品`
})

// 缩拼：汉字取拼音首字母（不带声调、不管多音字），字母/数字转小写，其它符号跳过；最多 20 位。
function genAbbr(name = '') {
  let out = ''
  for (const ch of String(name)) {
    if (out.length >= 20) break
    if (/[a-zA-Z0-9]/.test(ch)) out += ch.toLowerCase()
    else if (/[一-龥]/.test(ch)) out += pinyin(ch, { pattern: 'first', toneType: 'none' })
  }
  return out
}
// 输入名称实时生成缩拼（仅新增；编辑时缩拼锁定不可改）
watch(() => form.value.name, n => { if (!form.value.id) form.value.abbr = genAbbr(n) })

async function reload() {
  loading.value = true
  try {
    const [p, c] = await Promise.all([api('/api/admin/products'), api('/api/admin/categories')])
    products.value = p.products
    cats.value = c.categories
  } catch (e) {
    ElMessage.error('商品列表加载失败：' + e.message)
  } finally {
    loading.value = false
  }
}

function edit(p) {
  err.value = ''; picBlob.value = null
  revokePreview() // 每次进编辑/切换商品，释放旧的 blob preview
  // 二维码只在第一次打开弹窗时才生成（延迟加载，避免进入商品页就做 canvas 操作）
  if (!qrImg.value) loadQr()
  if (p) {
    form.value = { id: p.id, name: p.name, class: p.class, abbr: p.abbr, jhj: p.jhj, price: p.price }
    preview.value = p.pic ? imgUrl(p.pic) + '?t=' + imgT.value : ''
  } else {
    form.value = { id: null, name: '', class: cats.value[0]?.name || '', abbr: '', jhj: 0, price: 0 }
    preview.value = ''
  }
  editing.value = true
}

async function save() {
  err.value = ''
  try {
    const f = form.value
    // 依次校验：先名称 → 再缩拼 → 最后售价（不能为 0）
    if (!f.name.trim()) throw new Error('请填写商品名称')
    if (!f.id && !f.abbr.trim()) throw new Error('该商品名生成不出缩拼，请手动填写')
    if (f.price <= 0) throw new Error('售价不能为 0')
    // 先建商品/更新拿到 id（图片名不由前端指定，见下）
    const { id } = await post('/api/admin/product', { id: f.id, name: f.name, class: f.class, abbr: f.abbr, jhj: f.jhj, price: f.price })
    // 有图再按 id 传图：服务端用该商品「最终缩拼」命名并回填 gds_pic
    // （缩拼在建商品时已唯一化 whh→whh_1，同缩拼商品图片名不冲突，删除也不误删共用图）
    let imgFailed = null
    if (picBlob.value) {
      try {
        await upload('/api/admin/product/' + id + '/image', picBlob.value)
      } catch (e) { imgFailed = e.message }
    }
    // 商品已建好（即使图片失败）：关弹窗 + 提示，避免用户以为失败重复点保存建出重复商品
    editing.value = false
    imgT.value = Date.now()
    ElMessage.success(imgFailed
      ? `商品已保存，但图片上传失败（${imgFailed}），可重新点击编辑补传`
      : (f.id ? '商品已更新' : '商品已添加'))
    reload()
  } catch (ex) { err.value = ex.message }
}

async function toggle(p) {
  try {
    await post(`/api/admin/product/${p.id}/state`, { state: p.state ? 0 : 1 })
    reload()
  } catch (e) {
    // 开关绑定的是数据库里的状态，操作失败它会自动弹回原位，不会假装成功
    ElMessage.error((p.state ? '下架失败' : '上架失败') + '：' + e.message)
  }
}

async function del() {
  err.value = ''
  try {
    await delApi(`/api/admin/product/${form.value.id}`)
    editing.value = false
    ElMessage.success('商品已删除')
    reload()
  } catch (ex) { err.value = ex.message }
}

async function addCat() {
  catErr.value = ''
  if (!newCat.value.trim()) return
  try {
    await post('/api/admin/category', { name: newCat.value.trim() })
    newCat.value = ''
    ElMessage.success('分类已添加')
    reload()
  } catch (ex) { catErr.value = ex.message }
}
async function doRename(old) {
  catErr.value = ''
  try {
    await post('/api/admin/category', { name: old, rename_to: renameTo.value.trim() })
    renaming.value = ''
    ElMessage.success('分类已改名')
    reload()
  } catch (ex) { catErr.value = ex.message }
}
async function delCat(name) {
  catErr.value = ''
  try {
    await delApi(`/api/admin/category/${encodeURIComponent(name)}`)
    ElMessage.success('分类已删除')
    reload()
  } catch (ex) { catErr.value = ex.message }
}

onMounted(() => { reload() })
// 切走商品页（App.vue 的 v-if 卸载）时释放 blob preview / 图片 blob，防常驻内存泄漏
onBeforeUnmount(() => {
  revokePreview()
  picBlob.value = null
})
</script>

<style scoped>
.bar { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; margin-bottom: 14px; }
/* 状态筛选 / 搜索框 / 按钮之间统一用 .bar 的 gap 控制，去掉 Element Plus 组件自带的间隔 */
.bar .el-radio-group, .bar .el-radio-button, .bar .el-input, .bar .el-button { margin: 0; }
/* 分类胶囊：用 el-radio-button 的 Element Plus 默认样式，不改外观；
   只设 flex:1 让分类组占满剩余空间，把后面的筛选/搜索/按钮推到右边 */
.cats { display: inline-flex; flex: 1 1 auto; flex-wrap: wrap; white-space: normal; }
/* 分类/状态筛选胶囊：激活时去掉实心底色，改主色边框+主色文字（plain 观感）。
   Element 的激活色走 CSS 变量（--el-radio-button-checked-*），改变量即改激活态。
   注意要用全局样式 + .pgoods 限定，scoped/:deep 对子组件内部产物匹配不可靠。 */
.mgrid { display: grid; grid-template-columns: repeat(5, 1fr); gap: 16px; min-height: 200px; }
.mempty { grid-column: 1/-1; text-align: center; color: #9ca3af; font-size: 15px; padding: 60px 0; }
.mcard { background: #fff; border-radius: 6px; overflow: hidden; box-shadow: 0 2px 6px rgba(0,0,0,.12); position: relative; }
.mcard .pic { height: 170px; display: flex; align-items: center; justify-content: center; cursor: pointer; border-bottom: 1px solid #f0f0f0; position: relative; }
.mcard .pic img { max-width: 90%; max-height: 160px; object-fit: contain; }
.mcard .pic:hover::after { content: '✏️ 点击编辑'; position: absolute; inset: 0; background: rgba(0,0,0,.45); color: #fff; display: flex; align-items: center; justify-content: center; font-size: 16px; }
.mcard .pn { font-size: 16px; color: #0e7490; font-weight: bold; padding: 8px 10px 2px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.mcard .pp { padding: 2px 10px 10px; font-size: 14px; color: #666; display: flex; justify-content: space-between; }
.mcard .pp b { color: #f97316; font-size: 17px; }
.mcard.off .pic img { opacity: .35; filter: grayscale(1); }
.mcard.off .pn { color: #9ca3af; }
.sw { position: absolute; top: 8px; right: 8px; z-index: 2; }
/* 图片 + 扫码添加商品：两个文字都用 el-form-item 的 label（同款元素），外层 .picline 横排成一行 */
.picline { display: flex; align-items: flex-start; gap: 16px; margin-bottom: 18px; }
.picline .el-form-item { margin-bottom: 0; }
.picbox { width: 120px; height: 120px; box-sizing: border-box; border: 1px dashed #d1d5db; border-radius: 8px; display: flex; align-items: center; justify-content: center; cursor: pointer; font-size: 13px; color: #999; text-align: center; overflow: hidden; flex-shrink: 0; }
.picbox img { width: 100%; height: 100%; object-fit: cover; }
.qrimg { width: 120px; height: 120px; flex-shrink: 0; }
.catrow { display: flex; align-items: center; gap: 8px; padding: 8px 0; border-bottom: 1px solid #f0f0f0; font-size: 15px; }
</style>
