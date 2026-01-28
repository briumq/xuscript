const { performance } = require('perf_hooks');

function run(name, repeat, logic) {
    let min = Infinity;
    for (let r = 0; r < repeat; r++) {
        const start = performance.now();
        logic();
        const end = performance.now();
        const duration = end - start;
        if (duration < min) {
            min = duration;
        }
    }
    return Math.round(min);
}

function testLoop(N) {
    return run(`loop_${N}`, 7, () => {
        let count = 0;
        for (let r = 1; r <= N; r++) {
            count += 1;
        }
    });
}

function testDict(N) {
    return run(`dict_${N}`, 7, () => {
        const d = {};
        for (let i = 0; i < N; i++) {
            const k = "k" + i;
            d[k] = i;
        }
        let sum = 0;
        for (let i = 0; i < N; i++) {
            const k = "k" + i;
            sum += d[k];
        }
    });
}

function testDictHot(N) {
    return run(`dict_hot_${N}`, 7, () => {
        const d = {};
        for (let i = 0; i < N; i++) {
            d[i.toString()] = i;
        }
        for (let j = 0; j < N; j++) {
            const _v = d["0"];
        }
    });
}

function testDictIntKey(N) {
    // JS objects use string keys, but Map can use any key.
    // However, JS engines optimize integer keys in objects.
    return run(`dict_intkey_${N}`, 7, () => {
        const d = {};
        for (let i = 0; i < N; i++) {
            d[i] = i;
        }
        let sum = 0;
        for (let i = 0; i < N; i++) {
            sum += d[i];
        }
    });
}

function testString(N) {
    return run(`string_${N}`, 7, () => {
        let s = "";
        for (let i = 0; i < N; i++) {
            s += i;
            s += ",";
        }
    });
}

function testStringBuilder(N) {
    // JS doesn't have a built-in StringBuilder, array join is the common way
    return run(`string_builder_${N}`, 7, () => {
        const b = [];
        for (let i = 0; i < N; i++) {
            b.push(i);
            b.push(",");
        }
        const s = b.join("");
    });
}

class Foo {
    constructor(x) {
        this.x = x;
    }
    add(n) {
        return this.x + n;
    }
}

function testStructMethod(N) {
    return run(`struct_method_${N}`, 7, () => {
        const f = new Foo(1);
        for (let i = 0; i < N; i++) {
            const _v = f.add(i);
        }
    });
}

function testTryCatch(N) {
    return run(`try_catch_${N}`, 7, () => {
        let placeholder = 0;
        for (let i = 1; i <= N; i++) {
            try {
                throw "e";
            } catch (e) {
                if (e.length > 0) {
                    placeholder = 0;
                }
            } finally {
                placeholder = 0;
            }
        }
    });
}

const N = parseInt(process.env.BENCH_SCALE) || 10000;
console.log(`loop_${N}=${testLoop(N)}`);
console.log(`dict_${N}=${testDict(N)}`);
console.log(`dict_hot_${N}=${testDictHot(N)}`);
console.log(`dict_intkey_${N}=${testDictIntKey(N)}`);
console.log(`string_${N}=${testString(N)}`);
console.log(`string_builder_${N}=${testStringBuilder(N)}`);
console.log(`struct_method_${N}=${testStructMethod(N)}`);
console.log(`try_catch_${N}=${testTryCatch(N)}`);
