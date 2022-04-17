use chat_engine::{Channel, CommentInput, CommentOutput as Comment, Page};
use garcon::Delay;
use ic_agent::Agent;
use ic_cdk::api::time;
use ic_cdk::call;
use ic_cdk::export::candid::{CandidType, Deserialize, Nat};
use ic_cdk::export::Principal;
use ic_cdk_macros::{heartbeat, init, query, update};
use ic_utils::call::AsyncCall;
use ic_utils::interfaces::ManagementCanister;
use ic_utils::Canister;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;

// const WASM_CODE: [u8] = std::include_bytes!();

thread_local! {
    static CHANNELS: RefCell<HashMap<String, Channel>> = RefCell::new(HashMap::new());
    static NODE_INFO: RefCell<NodeInfo> = RefCell::new(NodeInfo::default());
}

#[derive(Clone, Debug, Default, CandidType, Deserialize)]
struct InstallArgs {
    node_info: Option<NodeInfo>,
}

#[export_name = "canister_init"]
fn init_canister(args: InstallArgs) {
    let mut current_node_info = NODE_INFO.with(|node_info| node_info.borrow_mut().clone());
    if let Some(node_info) = args.node_info {
        current_node_info = node_info;
        let index_node = current_node_info.index_node;
        // send message to index node to update its node info
    }
}

#[update]
fn upsert_comment(param: UpsertCommentParam) -> String {
    let user_id = ic_cdk::caller().to_string();
    match authenticate_user_and_comment_action(
        &param.channel_id,
        &param.comment_id,
        &user_id,
        |channel| {
            let comment_input = CommentInput {
                content: param.message.to_string(),
                id: param.comment_id.clone(),
                parent_id: param.parent_id.clone(),
                user_id: user_id.clone(),
                created_at: time(),
            };
            channel.upsert_comment(comment_input)
        },
    ) {
        Ok(result) => match result {
            Ok(output) => output.id,
            Err(message) => message,
        },
        Err(message) => message,
    }
}

#[update]
fn delete_comment(param: DeleteCommentParam) -> String {
    let user_id = ic_cdk::caller().to_string();
    let result = authenticate_user_and_comment_action(
        &param.channel_id,
        &param.comment_id,
        &user_id,
        |channel| {
            channel.delete_comment(param.comment_id.clone());
        },
    );

    match result {
        Ok(()) => "OK".to_string(),
        Err(message) => message,
    }
}

#[query]
fn get_thread(param: GetThreadParam) -> IPage {
    CHANNELS.with(|channels| {
        let mut channels = channels.borrow_mut();
        let channel = channels
            .entry(param.channel_id.to_string())
            .or_insert_with(|| Channel::new(param.channel_id.to_string()));
        let page = channel.get_page(&(param.limit as usize), param.cursor.as_ref());
        page.map(|p| p.into()).unwrap_or_default()
    })
}

#[query]
fn node_info() -> NodeInfoVec {
    NODE_INFO.with(|node_info| {
        let node_info = node_info.borrow();
        NodeInfoVec {
            all_nodes: node_info.all_nodes.into_iter().collect::<Vec<String>>(),
            index_node: node_info.index_node,
        }
    })
}

#[heartbeat]
fn heartbeat() -> () {}

fn authenticate_user_and_comment_action<A, T>(
    channel_id: &String,
    comment_id: &String,
    user_id: &String,
    action: A,
) -> Result<T, String>
where
    A: Fn(&mut Channel) -> T,
{
    CHANNELS.with(|channels| {
        let mut channels = channels.borrow_mut();
        let channel = channels
            .entry(channel_id.to_string())
            .or_insert_with(|| Channel::new(channel_id.to_string()));

        let message = match channel.get_comment(comment_id) {
            Some(comment) => {
                if &comment.user_id != user_id {
                    Err("UNAUTHORIZED".to_string())
                } else {
                    Ok(action(channel))
                }
            }
            None => Ok(action(channel)),
        };
        message
    })
}

fn install_code(
    management_canister: &Canister<ManagementCanister>,
    canister_id: Principal,
    waiter: &Delay,
    new_node_info: NodeInfo,
) -> Result<Principal, ()> {
    let result = management_canister
        .install_code(&canister_id, &WASM_CODE)
        .with_arg(InstallArgs {
            node_info: Some(new_node_info),
        })
        .call_and_wait(waiter)
        .await;

    result.map_err(|_| ()).map(|_| canister_id)
}

fn create_new_node(
    management_canister: &Canister<ManagementCanister>,
    waiter: &Delay,
) -> Result<Principal, ()> {
    match management_canister
        .create_canister()
        .with_controller(ic_cdk::id())
        .as_provisional_create_with_amount(Some(1_000_000))
        .build()
    {
        Ok(create_canister) => {
            create_canister
                .map(|canister_id| canister_id)
                .call_and_wait(waiter)
                .await
        }
        Err(err) => {
            println!("{:?}", err);
            Err(())
        }
    };
}

fn scale_up() {
    let agent = Agent::builder()
        .with_url(URL)
        .with_identity(create_identity())
        .build()?;

    let management_canister = ManagementCanister::create(&agent);

    let waiter = garcon::Delay::builder()
        .throttle(std::time::Duration::from_millis(500))
        .timeout(std::time::Duration::from_secs(60 * 5))
        .build();

    let result = create_new_node(&management_canister, &waiter).and_then(|canister_id| {
        install_code(
            &management_canister,
            canister_id,
            &waiter,
            NODE_INFO.with(|node_info| NodeInfo {
                index_node: node_info.borrow().index_node,
                all_nodes: node_info.borrow().all_nodes.clone().insert(canister_id),
            }),
        )
    });

    if Ok(canister_id) = result {
        NODE_INFO.with(|node_info| {
            let mut node_info = node_info.borrow_mut();
            node_info.all_nodes.insert(canister_id);
        });
    }
}

fn migrate_to_node(node_id: String) -> Result<(), ()> {
    Err(())
}

#[derive(Clone, Debug, Default, CandidType, Deserialize)]
struct UpsertCommentParam {
    channel_id: String,
    message: String,
    comment_id: String,
    parent_id: Option<String>,
}

#[derive(Clone, Debug, Default, CandidType, Deserialize)]
struct DeleteCommentParam {
    channel_id: String,
    comment_id: String,
}

#[derive(Clone, Debug, Default, CandidType, Deserialize)]
struct GetThreadParam {
    limit: u8,
    channel_id: String,
    cursor: Option<String>,
}

#[derive(Clone, Debug, Default, CandidType, Deserialize)]
struct ICommentOutput {
    id: String,
    content: String,
    created_at: Nat,
    user_id: String,
    replies: IPage,
}

#[derive(Clone, Debug, Default, CandidType, Deserialize)]
struct IPage {
    comments: Vec<ICommentOutput>,
    remaining_count: Nat,
}

impl From<Comment> for ICommentOutput {
    fn from(comment: Comment) -> Self {
        Self {
            id: comment.id,
            content: comment.content,
            created_at: Nat::from(comment.created_at),
            user_id: comment.user_id.to_string(),
            replies: comment.replies.into(),
        }
    }
}

impl From<Page> for IPage {
    fn from(page: Page) -> Self {
        IPage {
            comments: page
                .comments
                .into_iter()
                .map(|comment| comment.into())
                .collect(),
            remaining_count: Nat::from(page.remaining_count),
        }
    }
}

#[derive(Clone, Debug, CandidType, Deserialize, Default)]
struct NodeInfo {
    pub all_nodes: HashSet<String>,
    pub index_node: String,
    // pub status: NodeStatus
}

#[derive(Clone, Debug, CandidType, Deserialize, Default)]
struct NodeInfoVec {
    pub all_nodes: Vec<String>,
    pub index_node: String,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum NodeStatus {
    Creating,
    Running,
}

//dfx canister call rust_hello get_thread '(record {limit=10;channel_id="channel_1";cursor=null})'
// dfx canister call rust_hello upsert_comment '(record {channel_id="channel_1";message="hello";comment_id="comment_id_1"})'
