# Media notes

For publishing images and videos for VS Code and GitHub rendering, keep the following in mind.

- VS Code supports mp4 with limited codecs. See <https://code.visualstudio.com/api/extension-guides/webview#supported-media-formats>. For best compatibility, use H.264 video codec and remove audio (usually not needed for brief screen recordings).
- WebP images work well in VS Code and GitHub. They are broadly supported and significantly smaller than png files. However, using `ffmpeg` often resulted in images that wouldn't render in GitHub or VS Code. To avoid this, use the `cwebp` utility to convert PNG files to WebP format.

## Images

On Mac, you can install `cwebp` with Homebrew via `brew install webp`. For Windows, see <https://developers.google.com/speed/webp/download>.

For good compression and reasonable quality for screenshots, you can lower the quality and resolution using settings such as the below:

```bash
cwebp -q 80 -resize 1200 0 -sharpness 7 -af -m 6 image.png -o image.webp
```

Sharpness and auto-filtering (`-af`) can help preserve details in screenshots. The `-m 6` option uses the slowest but best compression method, which can significantly reduce file size while maintaining quality.

Put images in this directory and use an HTML tag in the markdown file to reference them similar to the below.

```text
<img width="668" alt="description" src="https://raw.githubusercontent.com/microsoft/qdk/main/media/filename.webp" />
```

## Video

For `ffmpeg`, you can use the following command to convert a video to mp4 format with the H.264 codec and no audio at 15 frames per second, and also resize the video to half the resolution while maintaining the aspect ratio. The quality (`-crf 28`) is good enough for screen recordings. You can add the `-ss` and `-to` options to trim the video start and end if needed.

```bash
ffmpeg -i input.mp4 -vf "scale=iw*0.5:-2:flags=lanczos,fps=15" -c:v libx264 -crf 28 -an output.mp4
```

For putting videos in the markdown file, use an HTML tag similar to the below:

```text
<video src="https://raw.githubusercontent.com/microsoft/qdk/main/media/filename.mp4" autoplay loop muted playsinline></video>
```

If you need help, all the LLMs are really good at generating `ffmpeg` and `cwebp` commands, so you can ask them to generate the command for you based on your specific needs, or explain some of the above flags further.
