# Denoiser benchmark

Scene `demo.scene` at 320 x 180. Every image is scored against a render of the same scene at 512 samples per pixel. PSNR and SSIM are computed on the 8-bit sRGB images, and higher is better. Regenerate with `prism-bench`.

| Samples | Denoised | PSNR (dB) | SSIM | Time (s) |
|---|---|---|---|---|
| 4 | no | 9.16 | 0.192 | 0.07 |
| 4 | yes | 9.60 | 0.273 | 0.19 |
| 16 | no | 20.67 | 0.536 | 0.14 |
| 16 | yes | 27.12 | 0.928 | 0.25 |
| 64 | no | 27.43 | 0.875 | 0.46 |
| 64 | yes | 29.67 | 0.954 | 0.57 |
