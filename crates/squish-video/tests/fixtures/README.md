# Test Fixtures

Small video files for integration tests. Generated with ffmpeg:

```bash
# 2-second 320x240 test pattern (MP4, ~100KB)
ffmpeg -f lavfi -i testsrc=duration=2:size=320x240:rate=15 -c:v libx264 -crf 28 sample.mp4

# WebM version
ffmpeg -f lavfi -i testsrc=duration=2:size=320x240:rate=15 -c:v libvpx-vp9 -crf 35 -b:v 0 sample.webm

# MOV version
ffmpeg -f lavfi -i testsrc=duration=2:size=320x240:rate=15 -c:v libx264 -crf 28 sample.mov
```

These are synthetic test patterns, not copyrighted content.
