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
      <el-button type="primary" @click="edit(null)">＋ 添加商品</el-button>
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

    <!-- 编辑/新增弹窗 -->
    <el-dialog v-model="editing" :title="form.id ? '编辑商品' : '添加商品'" width="480px">
      <el-form label-width="60px">
        <el-form-item label="图片">
          <div class="picrow">
            <div class="picbox" @click="fileInput.click()">
              <img v-if="preview" :src="preview">
              <span v-else>点击选择图片<br><small>自动裁剪成 300×300</small></span>
            </div>
            <div class="qrbox" v-if="qrImg" :title="mobileUrl">
              <img :src="qrImg">
              <span>手机扫码<br><small>打开手机端添加商品</small></span>
            </div>
          </div>
          <input ref="fileInput" type="file" accept="image/*" style="display:none" @change="onFile">
        </el-form-item>
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
        <el-button type="primary" @click="save">保存</el-button>
        <el-button @click="editing = false">取消</el-button>
        <el-popconfirm v-if="form.id" title="确定删除该商品？" @confirm="del">
          <template #reference>
            <el-button type="danger" style="float:left">删除</el-button>
          </template>
        </el-popconfirm>
      </template>
    </el-dialog>

    <!-- 分类管理弹窗 -->
    <el-dialog v-model="catDlg" title="分类管理" width="400px">
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
  </section>
</template>

<script setup>
import { ref, computed, onMounted, watch } from 'vue'
import { pinyin } from 'pinyin-pro'
import QRCode from 'qrcode'
import { api, post, del as delApi, upload, imgUrl, to300, PORT } from '../api'

const products = ref([]), cats = ref([]), loading = ref(true)
const kw = ref(''), cls = ref(''), stateFilter = ref('all'), imgT = ref(Date.now())
const editing = ref(false), form = ref({}), preview = ref(''), picBlob = ref(null), err = ref('')
const catDlg = ref(false), newCat = ref(''), catErr = ref(''), renaming = ref(''), renameTo = ref('')
const fileInput = ref(null)
// 手机端二维码：内容为本机局域网 IPv4 + 端口的 /m/ 页面地址，手机扫码直接打开
const qrImg = ref(''), mobileUrl = ref('')
async function loadQr() {
  try {
    const h = await api('/api/admin/hostinfo')
    mobileUrl.value = `http://${h.lan_ip}:${PORT}/m/`
    qrImg.value = await QRCode.toDataURL(mobileUrl.value, { width: 200, margin: 1 })
  } catch { /* 拿不到局域网 IP 时只隐藏二维码，不影响弹窗其它功能 */ }
}

const filtered = computed(() => products.value.filter(p =>
  (!cls.value || p.class === cls.value) &&
  (stateFilter.value === 'all' || (stateFilter.value === 'on' ? !!p.state : !p.state)) &&
  (!kw.value || p.name.includes(kw.value) || p.abbr.includes(kw.value.toLowerCase()))))

// 空状态提示：类别照打（「全部」→「全部商品」，具体分类 →「XX的商品」）；
// 状态只有「正在销售/停止销售」才打，「全部」不打
const emptyText = computed(() => {
  const cat = (cls.value || '全部') + '的商品'
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
  } finally {
    loading.value = false
  }
}

function edit(p) {
  err.value = ''; picBlob.value = null
  if (p) {
    form.value = { id: p.id, name: p.name, class: p.class, abbr: p.abbr, jhj: p.jhj, price: p.price }
    preview.value = p.pic ? imgUrl(p.pic) + '?t=' + imgT.value : ''
  } else {
    form.value = { id: null, name: '', class: cats.value[0]?.name || '', abbr: '', jhj: 0, price: 0 }
    preview.value = ''
  }
  editing.value = true
}

async function onFile(e) {
  const f = e.target.files[0]
  if (!f) return
  try {
    picBlob.value = await to300(f, 'image/jpeg')
    preview.value = URL.createObjectURL(picBlob.value)
  } catch (ex) { err.value = ex.message }
  e.target.value = ''
}

async function save() {
  err.value = ''
  try {
    const f = form.value
    // 依次校验：先名称 → 再缩拼 → 最后售价（不能为 0）
    if (!f.name.trim()) throw new Error('请填写商品名称')
    if (!f.id && !f.abbr.trim()) throw new Error('该商品名生成不出缩拼，请手动填写')
    if (f.price <= 0) throw new Error('售价不能为 0')
    let pic
    if (picBlob.value) {
      const name = (f.abbr || 'p' + Date.now()) + '.jpg'
      await upload('/api/admin/image/' + name, picBlob.value)
      pic = name
    }
    await post('/api/admin/product', { id: f.id, name: f.name, class: f.class, abbr: f.abbr, jhj: f.jhj, price: f.price, pic })
    editing.value = false
    imgT.value = Date.now()
    reload()
  } catch (ex) { err.value = ex.message }
}

async function toggle(p) {
  await post(`/api/admin/product/${p.id}/state`, { state: p.state ? 0 : 1 })
  reload()
}

async function del() {
  err.value = ''
  try {
    await delApi(`/api/admin/product/${form.value.id}`)
    editing.value = false
    reload()
  } catch (ex) { err.value = ex.message }
}

async function addCat() {
  catErr.value = ''
  if (!newCat.value.trim()) return
  try { await post('/api/admin/category', { name: newCat.value.trim() }); newCat.value = ''; reload() }
  catch (ex) { catErr.value = ex.message }
}
async function doRename(old) {
  catErr.value = ''
  try { await post('/api/admin/category', { name: old, rename_to: renameTo.value.trim() }); renaming.value = ''; reload() }
  catch (ex) { catErr.value = ex.message }
}
async function delCat(name) {
  catErr.value = ''
  try {
    await delApi(`/api/admin/category/${encodeURIComponent(name)}`)
    reload()
  } catch (ex) { catErr.value = ex.message }
}

onMounted(() => { reload(); loadQr() })
</script>

<style scoped>
.bar { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; margin-bottom: 14px; }
/* 状态筛选 / 搜索框 / 按钮之间统一用 .bar 的 gap 控制，去掉 Element Plus 组件自带的间隔 */
.bar .el-radio-group, .bar .el-radio-button, .bar .el-input, .bar .el-button { margin: 0; }
/* 分类胶囊：用 el-radio-button 的 Element Plus 默认样式，不改外观；
   只设 flex:1 让分类组占满剩余空间，把后面的筛选/搜索/按钮推到右边 */
.cats { display: inline-flex; flex: 1 1 auto; flex-wrap: wrap; white-space: normal; }
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
.picrow { display: flex; gap: 16px; align-items: stretch; }
.picbox { width: 120px; height: 120px; border: 1px dashed #d1d5db; border-radius: 8px; display: flex; align-items: center; justify-content: center; cursor: pointer; font-size: 13px; color: #999; text-align: center; overflow: hidden; }
.picbox img { max-width: 100%; max-height: 100%; object-fit: contain; }
.qrbox { width: 120px; border: 1px solid #ebeef5; border-radius: 8px; display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 6px; box-sizing: border-box; font-size: 13px; color: #999; text-align: center; }
.qrbox img { width: 84px; height: 84px; }
.catrow { display: flex; align-items: center; gap: 8px; padding: 8px 0; border-bottom: 1px solid #f0f0f0; font-size: 15px; }
</style>
