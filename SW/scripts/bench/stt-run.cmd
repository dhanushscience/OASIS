@rem Speech bench pinned to some CPUs:  stt-run.cmd <affinity hex> <model folder> <threads> <runs>
@rem   stt-run.cmd FF "%LOCALAPPDATA%\ProjectOASIS\Models\stt\parakeet-tdt-0.6b-v2" 4 3   (all cores)
@rem   stt-run.cmd 5  "...\parakeet-tdt-0.6b-v2" 2 3                                       (2 physical cores)
@rem Build first: cargo build --release -p oasis-voice --example bench_stt. Clips: clips\make.sh.
@start "" /b /wait /affinity %1 "%~dp0..\..\target\release\examples\bench_stt.exe" %2 "%~dp0..\..\target\debug\native" "%LOCALAPPDATA%\ProjectOASIS\bench\clips" %3 %4
