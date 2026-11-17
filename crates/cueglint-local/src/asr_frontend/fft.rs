// SPDX-License-Identifier: (MIT OR Apache-2.0) AND MIT
// FFT/DFT adaptation from Hugging Face Candle 0.11.1 (MIT OR Apache-2.0),
// VCS 59f8e903ca80a32ea7f8514b39748fc979e69e82,
// candle-transformers/src/models/whisper/audio.rs, SHA256
// ed5f1c81a0bebf0e67f8066f51d87cf8312a48aadb3a9a3a126ab7c09714880b.
// Modification by Cueglint: generic helpers specialized to f64; helper exported
// only to its parent module. Numerical helper body is otherwise retained.
// Candle attributes this code to whisper.cpp; the inherited MIT notice follows.
// Candle's own MIT and Apache-2.0 texts are separately preserved in
// assets/licenses/asr-frontend/. Ordinary new Cueglint feature assembly remains
// MIT OR Apache-2.0 in the sibling asr_frontend.rs module.
/*
MIT License

Copyright (c) 2023 Georgi Gerganov

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
*/

// https://github.com/ggerganov/whisper.cpp/blob/4774d2feb01a772a15de81ffc34b34a1f294f020/whisper.cpp#L2357
pub(super) fn fft(inp: &[f64]) -> Vec<f64> {
    let n = inp.len();
    let zero = 0.0;
    if n == 1 {
        return vec![inp[0], zero];
    }
    if n % 2 == 1 {
        return dft(inp);
    }
    let mut out = vec![zero; n * 2];

    let mut even = Vec::with_capacity(n / 2);
    let mut odd = Vec::with_capacity(n / 2);

    for (i, &inp) in inp.iter().enumerate() {
        if i % 2 == 0 {
            even.push(inp)
        } else {
            odd.push(inp);
        }
    }

    let even_fft = fft(&even);
    let odd_fft = fft(&odd);

    let two_pi = std::f64::consts::PI + std::f64::consts::PI;
    let n_t = n as f64;
    for k in 0..n / 2 {
        let k_t = k as f64;
        let theta = two_pi * k_t / n_t;
        let re = theta.cos();
        let im = -theta.sin();

        let re_odd = odd_fft[2 * k];
        let im_odd = odd_fft[2 * k + 1];

        out[2 * k] = even_fft[2 * k] + re * re_odd - im * im_odd;
        out[2 * k + 1] = even_fft[2 * k + 1] + re * im_odd + im * re_odd;

        out[2 * (k + n / 2)] = even_fft[2 * k] - re * re_odd + im * im_odd;
        out[2 * (k + n / 2) + 1] = even_fft[2 * k + 1] - re * im_odd - im * re_odd;
    }
    out
}

// https://github.com/ggerganov/whisper.cpp/blob/4774d2feb01a772a15de81ffc34b34a1f294f020/whisper.cpp#L2337
fn dft(inp: &[f64]) -> Vec<f64> {
    let zero = 0.0;
    let n = inp.len();
    let two_pi = std::f64::consts::PI + std::f64::consts::PI;

    let mut out = Vec::with_capacity(2 * n);
    let n_t = n as f64;
    for k in 0..n {
        let k_t = k as f64;
        let mut re = zero;
        let mut im = zero;

        for (j, &inp) in inp.iter().enumerate() {
            let j_t = j as f64;
            let angle = two_pi * k_t * j_t / n_t;
            re += inp * angle.cos();
            im -= inp * angle.sin();
        }

        out.push(re);
        out.push(im);
    }
    out
}
