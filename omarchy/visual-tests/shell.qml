import QtQuick
import QtQuick.Window
import Quickshell
import qs.Commons as Commons
import "Usage"

// Deterministic, offscreen visual scenarios for the real Quattro settings
// component. scripts/omarchy-visual-test.sh supplies the shell modules and
// output directory, then launches this config with Qt's offscreen backend.
ShellRoot {
  id: root

  readonly property string outputDir: Quickshell.env("AI_USAGEBAR_VISUAL_OUTPUT_DIR")
  property int scenario: 0
  property color scenarioBackground: "#1e1e2e"

  function applyTheme(foreground, background, accent, urgent, muted) {
    // FileView may finish loading the user's current theme after startup.
    // Reapply the fixture immediately before each capture so output is stable
    // and the test never depends on or mutates personal theme files.
    Commons.Color.themeShellValues = ({})
    Commons.Color.userShellValues = ({})
    Commons.Color.mergeShell()
    Commons.Color.foreground = foreground
    Commons.Color.background = background
    Commons.Color.accent = accent
    Commons.Color.urgent = urgent
    Commons.Color.muted = muted
    Commons.Style.cornerRadius = 8
  }

  function focusFirst(item) {
    if (!item) return false
    if (item.activeFocusOnTab === true && item.visible && item.enabled) {
      item.forceActiveFocus()
      return true
    }
    var children = item.children || []
    for (var i = 0; i < children.length; i++) {
      if (focusFirst(children[i])) return true
    }
    return false
  }

  function prepareScenario() {
    if (scenario === 0) {
      applyTheme("#cdd6f4", "#1e1e2e", "#89b4fa", "#f38ba8", "#6c7086")
      scenarioBackground = "#1e1e2e"
      settings.foreground = "#cdd6f4"
      settings.urgent = "#f38ba8"
      settings.openSection = "display"
      viewport.contentY = 0
    } else {
      applyTheme("#1f2328", "#f6f8fa", "#0969da", "#cf222e", "#656d76")
      scenarioBackground = "#f6f8fa"
      settings.foreground = "#1f2328"
      settings.urgent = "#cf222e"
      settings.openSection = "credentials"
      viewport.contentY = 0
    }
    settings.forceActiveFocus()
    Qt.callLater(function() { root.focusFirst(settings) })
    captureDelay.restart()
  }

  function capture() {
    var name = scenario === 0
      ? "settings-display-dark.png"
      : "settings-credentials-light.png"
    window.contentItem.grabToImage(function(result) {
      if (!result.saveToFile(outputDir + "/" + name)) {
        console.error("VISUAL SCENARIO FAIL: could not write " + name)
        Qt.exit(1)
        return
      }
      console.log("VISUAL SCENARIO OK: " + name)
      scenario++
      if (scenario < 2) prepareScenario()
      else Qt.quit()
    }, Qt.size(window.width, window.height))
  }

  Window {
    id: window
    width: 560
    height: 760
    visible: true
    title: "AI Usage visual scenarios"

    Rectangle {
      anchors.fill: parent
      color: root.scenarioBackground
    }

    Flickable {
      id: viewport
      anchors.fill: parent
      anchors.margins: 24
      clip: true
      contentWidth: width
      contentHeight: settings.implicitHeight

      SettingsView {
        id: settings
        width: viewport.width
        fontFamily: "monospace"
        showValue: true
        showProvider: true
        showAll: false
        colorCodeUsage: true
        brandIcons: true
        selectedPrimary: "anthropic"
        barProviderChoices: [
          { id: "anthropic", name: "Claude", checked: true },
          { id: "openai", name: "Codex", checked: true },
          { id: "openrouter", name: "OpenRouter", checked: false }
        ]
        metricEntries: [
          { id: "anthropic", name: "Claude", rows: [
            { key: "window:session", label: "Session", labelKey: "", group: "", checked: true, canToggle: true },
            { key: "window:weekly", label: "Weekly", labelKey: "", group: "", checked: true, canToggle: true }
          ] }
        ]
        snapshot: ({
          primary_choices: [
            { id: "anthropic", value: "anthropic", label: "Claude" },
            { id: "openai", value: "openai", label: "Codex" },
            { id: "openrouter", value: "openrouter", label: "OpenRouter" }
          ],
          vendors: [
            { id: "anthropic", label: "Claude", enabled: true },
            { id: "openai", label: "Codex", enabled: true },
            { id: "openrouter", label: "OpenRouter", enabled: false }
          ],
          keys: [
            { id: "openrouter", label: "OpenRouter", environment: "OPENROUTER_API_KEY",
              secret_label: "API key", note: "Optional explicit credential",
              configured: true, inline_configured: true, environment_configured: false },
            { id: "zai", label: "Z.AI", environment: "ZAI_API_KEY",
              secret_label: "API key", note: "Usage API access",
              configured: false, inline_configured: false, environment_configured: false }
          ]
        })
      }
    }
  }

  Timer {
    id: startup
    interval: 150
    running: true
    repeat: false
    onTriggered: root.prepareScenario()
  }

  Timer {
    id: captureDelay
    interval: 180
    repeat: false
    onTriggered: root.capture()
  }

  Timer {
    interval: 10000
    running: true
    repeat: false
    onTriggered: {
      console.error("VISUAL SCENARIO FAIL: timed out")
      Qt.exit(1)
    }
  }
}
