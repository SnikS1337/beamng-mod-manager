-- ModManager BeamNG: загрузить помощник и держать его загруженным
extensions.load('bmmBridge')
pcall(setExtensionUnloadMode, 'bmmBridge', 'manual')
