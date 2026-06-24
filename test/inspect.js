const m = require('../index.node');
console.log('keys:', Object.keys(m));
for (const k of Object.keys(m)) {
  console.log(k, typeof m[k]);
}
