use super::*;

pub async fn sync_blocks<B: Blockchain>(
    context: &Arc<RwLock<NodeContext<B>>>,
) -> Result<(), NodeError> {
    let ctx = context.read().await;
    let power = ctx.blockchain.get_power()?;
    let net = ctx.outgoing.clone();

    let height = ctx.blockchain.get_height()?;

    // Find the peer that claims the highest power.
    let most_powerful = ctx
        .active_peers()
        .into_iter()
        .max_by_key(|p| p.info.as_ref().map(|i| i.power).unwrap_or(0))
        .ok_or(NodeError::NoPeers)?;
    drop(ctx);

    let most_powerful_info = most_powerful.info.as_ref().ok_or(NodeError::NoPeers)?;

    if most_powerful_info.power <= power {
        return Ok(());
    }

    let start_height = std::cmp::min(height, most_powerful_info.height);

    // Get all headers starting from the indices that we don't have.
    let mut headers = net
        .bincode_get::<GetHeadersRequest, GetHeadersResponse>(
            format!("{}/bincode/headers", most_powerful.address),
            GetHeadersRequest {
                since: start_height,
                until: None,
            },
        )
        .await?
        .headers;

    // The local blockchain and the peer blockchain both have all blocks
    // from 0 to height-1, though, the blocks might not be equal. Find
    // the header from which the fork has happened.
    for index in (0..start_height).rev() {
        let peer_header = net
            .bincode_get::<GetHeadersRequest, GetHeadersResponse>(
                format!("{}/bincode/headers", most_powerful.address),
                GetHeadersRequest {
                    since: index,
                    until: Some(index + 1),
                },
            )
            .await?
            .headers[0]
            .clone();

        let ctx = context.read().await;
        let local_header = ctx.blockchain.get_headers(index, Some(index + 1))?[0].clone();
        drop(ctx);

        if local_header.hash() != peer_header.hash() {
            headers.insert(0, peer_header);
        } else {
            break;
        }
    }

    let will_extend = {
        let ctx = context.read().await;
        ctx.blockchain
            .will_extend(headers[0].number as usize, &headers)
            .unwrap_or(false)
    };

    if will_extend {
        let resp = net
            .bincode_get::<GetBlocksRequest, GetBlocksResponse>(
                format!("{}/bincode/blocks", most_powerful.address).to_string(),
                GetBlocksRequest {
                    since: headers[0].number as usize,
                    until: None,
                },
            )
            .await?;
        let mut ctx = context.write().await;
        ctx.blockchain
            .extend(headers[0].number as usize, &resp.blocks)?;
    }

    Ok(())
}
