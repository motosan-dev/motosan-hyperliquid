use hl_types::HlError;

use super::parse::WsMessage;
use super::HyperliquidWs;

impl HyperliquidWs {
    /// Convert this WebSocket client into a [`futures_util::Stream`] of typed
    /// [`WsMessage`]s.
    ///
    /// This consumes the `HyperliquidWs` and returns an opaque stream that
    /// yields `Result<WsMessage, HlError>` items until the connection is
    /// permanently closed.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use futures_util::StreamExt;
    ///
    /// # async fn example() -> Result<(), hl_types::HlError> {
    /// let mut ws = hl_client::HyperliquidWs::mainnet();
    /// ws.connect().await?;
    /// let stream = ws.into_stream();
    /// tokio::pin!(stream);
    /// while let Some(msg) = stream.next().await {
    ///     println!("{:?}", msg?);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn into_stream(self) -> impl futures_util::Stream<Item = Result<WsMessage, HlError>> {
        futures_util::stream::unfold(self, |mut ws| async move {
            let msg = ws.next_typed_message().await;
            msg.map(|result| (result, ws))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::WsConfig;
    use super::*;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn into_stream_yields_error_on_cancelled_token() {
        use futures_util::StreamExt;

        let token = CancellationToken::new();
        token.cancel();

        let config = WsConfig::with_cancellation_token(token);
        let ws = HyperliquidWs::with_config("wss://127.0.0.1:1".to_string(), config);

        let stream = ws.into_stream();
        tokio::pin!(stream);
        let item = stream.next().await;
        assert!(item.is_some());
        let err = item.unwrap().unwrap_err();
        assert!(
            matches!(err, HlError::WsCancelled),
            "expected WsCancelled from stream, got: {err:?}"
        );
    }
}
