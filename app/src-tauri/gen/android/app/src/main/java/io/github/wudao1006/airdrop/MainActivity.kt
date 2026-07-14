package io.github.wudao1006.airdrop

import android.content.Context
import android.net.wifi.WifiManager
import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  private var multicastLock: WifiManager.MulticastLock? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    acquireMulticastLock()
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  override fun onResume() {
    acquireMulticastLock()
    super.onResume()
  }

  override fun onPause() {
    super.onPause()
    releaseMulticastLock()
  }

  override fun onDestroy() {
    releaseMulticastLock()
    super.onDestroy()
  }

  private fun acquireMulticastLock() {
    val current = multicastLock
    if (current?.isHeld == true) return
    val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager ?: return
    multicastLock = runCatching {
      wifi.createMulticastLock("airdrop-mdns").apply {
        setReferenceCounted(false)
        acquire()
      }
    }.getOrNull()
  }

  private fun releaseMulticastLock() {
    multicastLock?.let { lock ->
      runCatching {
        if (lock.isHeld) lock.release()
      }
    }
    multicastLock = null
  }
}
