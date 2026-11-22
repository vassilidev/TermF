# ffmpeg recipes

Read when you encode frames, make a GIF, add music or subtitles. Paths are relative to the video project.

## Frames to MP4 (every platform)

```sh
ffmpeg -framerate 30 -i frames/f%05d.png \
  -vf "scale=trunc(iw/2)*2:trunc(ih/2)*2" \
  -c:v libx264 -pix_fmt yuv420p -crf 18 -preset slow -movflags +faststart out/launch-16x9.mp4
```

`-crf 18` is near transparent; raise it (20 to 23) when the file is too heavy. `-pattern_type glob -i 'frames/*.png'` reads frames whose numbers are not contiguous.

## Music bed

```sh
# 1. Measure
ffmpeg -i media/music.mp3 -af loudnorm=I=-14:TP=-1:LRA=11:print_format=json -f null -
# 2. Apply with the measured values, fade in 1 s and out 2 s for a 20 s video, and mux with the picture
ffmpeg -i out/launch-16x9.mp4 -i media/music.mp3 \
  -af "loudnorm=I=-14:TP=-1:LRA=11:measured_I=<I>:measured_TP=<TP>:measured_LRA=<LRA>:measured_thresh=<thresh>:linear=true,afade=t=in:st=0:d=1,afade=t=out:st=18:d=2" \
  -ar 48000 -c:v copy -c:a aac -b:a 192k -shortest -movflags +faststart out/launch-16x9-sound.mp4
```

Platforms publish no loudness target for social video; -14 LUFS with true peak under -1 dB is what streaming services use, a sensible default.

## Subtitles

```sh
ffmpeg -filters | grep subtitles                          # burned-in text needs libass
ffmpeg -i out/launch-9x16.mp4 -vf "subtitles=out/launch.srt:force_style='Fontsize=22,MarginV=120'" \
  -c:v libx264 -pix_fmt yuv420p -crf 18 -c:a copy -movflags +faststart out/launch-9x16-subtitled.mp4
ffmpeg -i out/launch-16x9.mp4 -i out/launch.srt -c copy -c:s mov_text out/launch-16x9-cc.mp4   # soft, toggleable
```

An SRT cue: index, `00:00:01,200 --> 00:00:03,000`, the text, a blank line.

## GIF

```sh
ffmpeg -ss 2 -t 4 -i out/launch-16x9.mp4 \
  -filter_complex "fps=15,scale=720:-1:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle" \
  -loop 0 out/launch.gif
```

Fewer frames per second, a smaller width or a shorter clip make it lighter; a GIF has no sound.

## Poster and stills

```sh
ffmpeg -ss 6.5 -i out/launch-16x9.mp4 -frames:v 1 out/poster.jpg        # a settled frame
ffmpeg -i out/launch-16x9.mp4 -vf "select='eq(n,90)+eq(n,195)'" -vsync 0 out/still-%02d.png   # frames by number
```

## Check

```sh
ffprobe -v error -show_entries format=duration,size:stream=codec_name,width,height,pix_fmt,r_frame_rate -of default=nw=1 out/launch-16x9.mp4
ffprobe -v error -count_frames -select_streams v:0 -show_entries stream=nb_read_frames -of csv=p=0 out/launch-16x9.mp4
```

## Sizes per format

| Format | Frame | Use |
|---|---|---|
| 16:9 | 1920 × 1080 | site, YouTube, X, LinkedIn |
| 9:16 | 1080 × 1920 | Reels, TikTok, Shorts (text in the middle band) |
| 1:1 | 1080 × 1080 | feeds |
| GIF | 720 wide at most | README, docs, chat |

X accepts H.264 High profile with 4:2:0 colour and AAC-LC audio, at most 60 fps; LinkedIn takes 3 seconds to 15 minutes, up to 5 GB. Check the platform's help page before a long or heavy upload.
